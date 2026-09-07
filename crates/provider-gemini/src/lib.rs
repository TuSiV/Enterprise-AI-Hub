// Copyright 2026 YONGZHE CHEN
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Google Gemini API Provider Adapter（方案 §10：厂商协议只存在于 Adapter 内）。
//! 端点：POST {base}/v1beta/models/{model}:generateContent、
//! :streamGenerateContent?alt=sse、:batchEmbedContents、GET /v1beta/models。
//! 鉴权：x-goog-api-key 头。

use std::time::{Duration, Instant};

use aihub_domain::canonical::{
    CanonicalChatRequest, CanonicalChatResponse, CanonicalEmbeddingRequest,
    CanonicalEmbeddingResponse, CanonicalUsage, DiscoveredModel, MessageRole, StreamEvent,
    ToolCallOutput, ToolChoice, UsageSource,
};
use aihub_domain::entities::ProviderKind;
use aihub_domain::error::ErrorCategory;
use aihub_provider_core::{
    ProviderError, ProviderFactory, ProviderHealth, ProviderResult, ProviderRuntimeConfig,
    ProviderStream,
};
use aihub_secrets::SecretValue;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};

pub struct GeminiFactory;

#[async_trait]
impl ProviderFactory for GeminiFactory {
    fn protocol(&self) -> &'static str {
        "gemini"
    }

    fn build(
        &self,
        config: ProviderRuntimeConfig,
    ) -> ProviderResult<std::sync::Arc<dyn aihub_provider_core::ModelProvider>> {
        Ok(std::sync::Arc::new(GeminiProvider::new(config)))
    }
}

pub struct GeminiProvider {
    http: reqwest::Client,
    base_url: String,
    credential: Option<SecretValue>,
    kind: ProviderKind,
    provider_key: String,
}

impl GeminiProvider {
    pub fn new(config: ProviderRuntimeConfig) -> Self {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_millis(config.timeout_ms.max(1000) as u64))
            .user_agent(concat!("aihub-gateway/", env!("CARGO_PKG_VERSION")));
        if let Some(proxy) = &config.proxy_url {
            if let Ok(p) = reqwest::Proxy::all(proxy) {
                builder = builder.proxy(p);
            }
        }
        Self {
            http: builder.build().unwrap_or_default(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            credential: config.credential,
            kind: config.kind,
            provider_key: config.provider_key,
        }
    }

    /// base 兼容 `https://generativelanguage.googleapis.com` 与 `.../v1beta` 两种填法。
    fn v1beta(&self, path: &str) -> String {
        if self.base_url.ends_with("/v1beta") {
            format!("{}{path}", self.base_url)
        } else {
            format!("{}/v1beta{path}", self.base_url)
        }
    }

    fn model_id(model: &str) -> String {
        model
            .trim_start_matches("models/")
            .trim_start_matches("tunedModels/")
            .to_string()
    }

    fn auth_request(&self, method: reqwest::Method, url: String) -> reqwest::RequestBuilder {
        let mut rb = self.http.request(method, url);
        if let Some(secret) = &self.credential {
            if !secret.is_empty() {
                rb = rb.header("x-goog-api-key", secret.expose());
            }
        }
        rb
    }

    fn transport_error(err: reqwest::Error) -> ProviderError {
        if err.is_timeout() {
            ProviderError::new(
                ErrorCategory::Timeout,
                format!("provider request timeout: {err}"),
            )
        } else if err.is_connect() {
            ProviderError::new(
                ErrorCategory::Connection,
                format!("provider connection failed: {err}"),
            )
        } else {
            ProviderError::new(
                ErrorCategory::Connection,
                format!("provider request failed: {err}"),
            )
        }
    }

    async fn map_status_error(&self, status: u16, body: String) -> ProviderError {
        let message = extract_error_message(&body);
        let mut err = match status {
            401 | 403 => ProviderError::new(ErrorCategory::Authentication, message.clone()),
            404 => ProviderError::new(ErrorCategory::ModelNotFound, message.clone()),
            429 => ProviderError::new(ErrorCategory::RateLimited, message.clone()),
            500..=599 => ProviderError::new(ErrorCategory::Provider5xx, message.clone()),
            _ => {
                let lower = message.to_lowercase();
                let category =
                    if lower.contains("token count") || lower.contains("exceeds the maximum") {
                        ErrorCategory::ContextLengthExceeded
                    } else if lower.contains("safety") {
                        ErrorCategory::ContentFiltered
                    } else {
                        ErrorCategory::InvalidRequest
                    };
                ProviderError::new(category, message.clone())
            }
        };
        err = err.with_status(status);
        if status == 429 {
            err = err.with_retry_after(1000);
        }
        err
    }

    /// Canonical → generateContent 请求体：system 抽取为 systemInstruction、
    /// tool 历史映射为 functionCall / functionResponse parts。
    fn build_generate_body(&self, request: &CanonicalChatRequest, stream: bool) -> (String, Value) {
        let model_id = Self::model_id(&request.model);
        let mut system_parts: Vec<String> = Vec::new();
        let mut contents: Vec<Value> = Vec::new();
        let mut call_seq: usize = 0;

        for m in &request.messages {
            match m.role {
                MessageRole::System => system_parts.push(m.content.clone()),
                MessageRole::Tool => {
                    let name = m.name.clone().unwrap_or_else(|| "tool".into());
                    // response 必须是 JSON 对象；content 为纯文本时包一层
                    let payload: Value = serde_json::from_str(&m.content)
                        .unwrap_or_else(|_| json!({"result": m.content}));
                    push_part(
                        &mut contents,
                        "user",
                        json!({"functionResponse": {"name": name, "response": payload}}),
                    );
                }
                MessageRole::Assistant => {
                    if !m.content.is_empty() {
                        push_part(&mut contents, "model", json!({"text": m.content}));
                    }
                    for call in m.tool_calls.iter().flatten() {
                        let args = serde_json::from_str::<Value>(&call.arguments)
                            .unwrap_or_else(|_| json!({}));
                        push_part(
                            &mut contents,
                            "model",
                            json!({"functionCall": {"name": call.name, "args": args}}),
                        );
                        call_seq += 1;
                    }
                }
                MessageRole::User => {
                    push_part(&mut contents, "user", json!({"text": m.content}));
                }
            }
        }
        let _ = call_seq;

        let mut body = json!({"contents": contents});
        if !system_parts.is_empty() {
            body["systemInstruction"] = json!({"parts": [{"text": system_parts.join("\n\n")}]});
        }

        let mut generation_config = json!({});
        if let Some(t) = request.temperature {
            generation_config["temperature"] = json!(t);
        }
        if let Some(p) = request.top_p {
            generation_config["topP"] = json!(p);
        }
        if let Some(max) = request.max_output_tokens {
            generation_config["maxOutputTokens"] = json!(max);
        }
        if let Some(rf) = &request.response_format {
            if matches!(rf, aihub_domain::canonical::ResponseFormat::Json) {
                generation_config["responseMimeType"] = json!("application/json");
            }
        }
        if generation_config
            .as_object()
            .map(|o| !o.is_empty())
            .unwrap_or(false)
        {
            body["generationConfig"] = generation_config;
        }

        if !request.tools.is_empty() {
            let declarations: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    let mut decl = json!({
                        "name": t.name,
                        "parameters": t.parameters.clone().unwrap_or(json!({"type": "object"})),
                    });
                    if let Some(desc) = &t.description {
                        decl["description"] = json!(desc);
                    }
                    decl
                })
                .collect();
            body["tools"] = json!([{"functionDeclarations": declarations}]);
            if let Some(choice) = &request.tool_choice {
                let (mode, allowed) = match choice {
                    ToolChoice::Auto => ("AUTO", None),
                    ToolChoice::None => ("NONE", None),
                    ToolChoice::Required => ("ANY", None),
                    ToolChoice::Function(name) => ("ANY", Some(name.clone())),
                };
                let mut tool_config = json!({"functionCallingConfig": {"mode": mode}});
                if let Some(name) = allowed {
                    tool_config["functionCallingConfig"]["allowedFunctionNames"] = json!([name]);
                }
                body["toolConfig"] = tool_config;
            }
        }

        let path = if stream {
            format!("/models/{model_id}:streamGenerateContent?alt=sse")
        } else {
            format!("/models/{model_id}:generateContent")
        };
        (path, body)
    }

    /// generateContent 响应 → Canonical。
    fn response_to_canonical(resp: &Value) -> CanonicalChatResponse {
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();
        let mut call_index = 0usize;

        let candidate = resp.get("candidates").and_then(|c| c.get(0));
        if let Some(parts) = candidate
            .and_then(|c| c.pointer("/content/parts"))
            .and_then(|p| p.as_array())
        {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    if part
                        .get("thought")
                        .and_then(|t| t.as_bool())
                        .unwrap_or(false)
                    {
                        reasoning.push_str(text);
                    } else {
                        content.push_str(text);
                    }
                }
                if let Some(call) = part.get("functionCall") {
                    tool_calls.push(ToolCallOutput {
                        id: format!("call_{call_index}"),
                        name: call
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        arguments: call
                            .get("args")
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "{}".into()),
                    });
                    call_index += 1;
                }
            }
        }
        let finish_reason = candidate
            .and_then(|c| c.get("finishReason"))
            .and_then(|v| v.as_str())
            .map(map_finish_reason);
        CanonicalChatResponse {
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            reasoning_content: if reasoning.is_empty() {
                None
            } else {
                Some(reasoning)
            },
            tool_calls,
            finish_reason,
            usage: resp.get("usageMetadata").map(Self::map_usage),
            provider_model: resp
                .get("modelVersion")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }

    fn map_usage(usage: &Value) -> CanonicalUsage {
        let input = usage
            .get("promptTokenCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output = usage
            .get("candidatesTokenCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let reasoning = usage
            .get("thoughtsTokenCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        CanonicalUsage {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: usage
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            reasoning_tokens: reasoning,
            total_tokens: usage
                .get("totalTokenCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(input + output),
            source: UsageSource::Provider,
        }
    }
}

/// parts 追加到最近一条同角色 content；否则新开一条（Gemini 要求 user/model 交替）。
fn push_part(contents: &mut Vec<Value>, role: &str, part: Value) {
    if let Some(last) = contents.last_mut() {
        if last.get("role").and_then(|r| r.as_str()) == Some(role) {
            last["parts"].as_array_mut().unwrap().push(part);
            return;
        }
    }
    contents.push(json!({"role": role, "parts": [part]}));
}

fn map_finish_reason(reason: &str) -> String {
    match reason {
        "STOP" => "stop".into(),
        "MAX_TOKENS" => "length".into(),
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
            "content_filter".into()
        }
        "MALFORMED_FUNCTION_CALL" => "tool_calls".into(),
        other => other.to_lowercase(),
    }
}

fn extract_error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(message) = value.pointer("/error/message").and_then(|m| m.as_str()) {
            return message.to_string();
        }
    }
    let truncated: String = body.chars().take(300).collect();
    if truncated.is_empty() {
        "provider returned an error".to_string()
    } else {
        truncated
    }
}

#[async_trait]
impl aihub_provider_core::ModelProvider for GeminiProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    async fn list_models(&self) -> ProviderResult<Vec<DiscoveredModel>> {
        let mut page_token: Option<String> = None;
        let mut models = Vec::new();
        for _ in 0..10 {
            let mut url = format!("{}?pageSize=1000", self.v1beta("/models"));
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={token}"));
            }
            let response = self
                .auth_request(reqwest::Method::GET, url)
                .send()
                .await
                .map_err(GeminiProvider::transport_error)?;
            let status = response.status().as_u16();
            if !response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(self.map_status_error(status, body).await);
            }
            let parsed: Value = response.json().await.map_err(|e| {
                ProviderError::new(
                    ErrorCategory::MalformedResponse,
                    format!("invalid /models response: {e}"),
                )
            })?;
            if let Some(items) = parsed.get("models").and_then(|m| m.as_array()) {
                for m in items {
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                    let key = name.strip_prefix("models/").unwrap_or(name);
                    if key.is_empty() {
                        continue;
                    }
                    let methods = m
                        .get("supportedGenerationMethods")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    let model_type = if methods.contains(&"embedContent") {
                        "embedding"
                    } else if methods.contains(&"generateContent") {
                        "chat"
                    } else {
                        continue;
                    };
                    models.push(DiscoveredModel {
                        model_key: key.to_string(),
                        display_name: m
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        model_type: Some(model_type.to_string()),
                        context_window: m.get("inputTokenLimit").and_then(|v| v.as_i64()),
                        capabilities: None,
                    });
                }
            }
            page_token = parsed
                .get("nextPageToken")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if page_token.is_none() {
                break;
            }
        }
        Ok(models)
    }

    async fn chat(&self, request: CanonicalChatRequest) -> ProviderResult<CanonicalChatResponse> {
        let (path, body) = self.build_generate_body(&request, false);
        let response = self
            .auth_request(reqwest::Method::POST, self.v1beta(&path))
            .json(&body)
            .send()
            .await
            .map_err(GeminiProvider::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: Value = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid generateContent response: {e}"),
            )
        })?;
        Ok(Self::response_to_canonical(&parsed))
    }

    async fn chat_stream(&self, request: CanonicalChatRequest) -> ProviderResult<ProviderStream> {
        let (path, body) = self.build_generate_body(&request, true);
        let response = self
            .auth_request(reqwest::Method::POST, self.v1beta(&path))
            .json(&body)
            .send()
            .await
            .map_err(GeminiProvider::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }

        let byte_stream = response.bytes_stream();
        let stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut chunks = byte_stream;
            let mut finish_reason: Option<String> = None;
            let mut usage: Option<CanonicalUsage> = None;
            let mut started = false;
            let mut tool_index: usize = 0;
            let mut emitted_error: Option<ProviderError> = None;

            while let Some(item) = chunks.next().await {
                match item {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find("\n\n") {
                            let block: String = buffer.drain(..pos + 2).collect();
                            for raw_line in block.lines() {
                                let line = raw_line.trim();
                                let Some(data) = line.strip_prefix("data:") else { continue };
                                let data = data.trim();
                                if data.is_empty() || data == "[DONE]" {
                                    continue;
                                }
                                let Ok(chunk) = serde_json::from_str::<Value>(data) else { continue };
                                if !started {
                                    started = true;
                                    let model = chunk.get("modelVersion").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    yield StreamEvent::ResponseStarted { provider_model: model };
                                }
                                if let Some(u) = chunk.get("usageMetadata") {
                                    let mut canonical = GeminiProvider::map_usage(u);
                                    if let Some(existing) = &usage {
                                        canonical.input_tokens = canonical.input_tokens.max(existing.input_tokens);
                                        canonical.output_tokens = canonical.output_tokens.max(existing.output_tokens);
                                        canonical.total_tokens = canonical.total_tokens.max(existing.total_tokens);
                                    }
                                    usage = Some(canonical.clone());
                                    yield StreamEvent::UsageUpdated { usage: canonical };
                                }
                                if let Some(reason) = chunk
                                    .pointer("/candidates/0/finishReason")
                                    .and_then(|v| v.as_str())
                                {
                                    finish_reason = Some(map_finish_reason(reason));
                                }
                                let parts = chunk
                                    .pointer("/candidates/0/content/parts")
                                    .and_then(|p| p.as_array())
                                    .cloned()
                                    .unwrap_or_default();
                                for part in parts {
                                    let is_thought = part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
                                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                        if !text.is_empty() {
                                            if is_thought {
                                                yield StreamEvent::ReasoningDelta { delta: text.to_string() };
                                            } else {
                                                yield StreamEvent::ContentDelta { delta: text.to_string() };
                                            }
                                        }
                                    }
                                    if let Some(call) = part.get("functionCall") {
                                        let name = call.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                                        let args = call.get("args").map(|v| v.to_string()).unwrap_or_else(|| "{}".into());
                                        yield StreamEvent::ToolCallStarted {
                                            index: tool_index,
                                            id: format!("call_{tool_index}"),
                                            name,
                                        };
                                        if !args.is_empty() && args != "{}" {
                                            yield StreamEvent::ToolCallArgumentsDelta {
                                                index: tool_index,
                                                delta: args,
                                            };
                                        }
                                        tool_index += 1;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        emitted_error = Some(GeminiProvider::transport_error(e));
                        break;
                    }
                }
            }

            if let Some(err) = emitted_error {
                yield StreamEvent::ProviderError {
                    category: err.category,
                    message: err.message,
                    http_status: err.http_status,
                };
                return;
            }
            yield StreamEvent::ResponseCompleted { finish_reason, usage };
        };
        Ok(Box::pin(stream))
    }

    async fn embeddings(
        &self,
        request: CanonicalEmbeddingRequest,
    ) -> ProviderResult<CanonicalEmbeddingResponse> {
        let model_id = Self::model_id(&request.model);
        let body = json!({
            "requests": request.inputs.iter().map(|input| json!({
                "model": format!("models/{model_id}"),
                "content": {"parts": [{"text": input}]},
            })).collect::<Vec<_>>(),
        });
        let path = format!("/models/{model_id}:batchEmbedContents");
        let response = self
            .auth_request(reqwest::Method::POST, self.v1beta(&path))
            .json(&body)
            .send()
            .await
            .map_err(GeminiProvider::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: Value = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid batchEmbedContents response: {e}"),
            )
        })?;
        let embeddings = parsed
            .get("embeddings")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        item.get("values")
                            .and_then(|v| v.as_array())
                            .map(|vec| {
                                vec.iter()
                                    .filter_map(|x| x.as_f64().map(|f| f as f32))
                                    .collect::<Vec<f32>>()
                            })
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(CanonicalEmbeddingResponse {
            embeddings,
            usage: None,
        })
    }

    async fn health_check(&self) -> ProviderResult<ProviderHealth> {
        let started = Instant::now();
        let mut rb = self
            .http
            .request(reqwest::Method::GET, self.v1beta("/models"))
            .timeout(Duration::from_secs(10));
        if let Some(secret) = &self.credential {
            if !secret.is_empty() {
                rb = rb.header("x-goog-api-key", secret.expose());
            }
        }
        match rb.send().await {
            Ok(resp) => {
                let latency = started.elapsed().as_millis() as i64;
                let status = resp.status().as_u16();
                if resp.status().is_success() {
                    Ok(ProviderHealth {
                        status: "healthy".into(),
                        latency_ms: Some(latency),
                        http_status: Some(status),
                        error_category: None,
                        message: None,
                    })
                } else {
                    Ok(ProviderHealth {
                        status: "unavailable".into(),
                        latency_ms: Some(latency),
                        http_status: Some(status),
                        error_category: Some(match status {
                            401 | 403 => ErrorCategory::Authentication,
                            429 => ErrorCategory::RateLimited,
                            500..=599 => ErrorCategory::Provider5xx,
                            _ => ErrorCategory::InvalidRequest,
                        }),
                        message: None,
                    })
                }
            }
            Err(e) => {
                let category = if e.is_timeout() {
                    ErrorCategory::Timeout
                } else {
                    ErrorCategory::Connection
                };
                Ok(ProviderHealth {
                    status: "unavailable".into(),
                    latency_ms: Some(started.elapsed().as_millis() as i64),
                    http_status: None,
                    error_category: Some(category),
                    message: Some(format!("{e}")),
                })
            }
        }
    }
}

/// 供 application 层查询 provider 标识（用于日志/审计）。
impl GeminiProvider {
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aihub_domain::canonical::{CanonicalMessage, ToolDefinitionData};

    fn request_fixture() -> CanonicalChatRequest {
        CanonicalChatRequest {
            model: "gemini-2.5-flash".into(),
            messages: vec![
                CanonicalMessage {
                    role: MessageRole::System,
                    content: "be brief".into(),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
                CanonicalMessage {
                    role: MessageRole::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
                CanonicalMessage {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    tool_call_id: None,
                    name: None,
                    tool_calls: Some(vec![ToolCallOutput {
                        id: "call_0".into(),
                        name: "get_weather".into(),
                        arguments: r#"{"city":"上海"}"#.into(),
                    }]),
                },
                CanonicalMessage {
                    role: MessageRole::Tool,
                    content: "sunny".into(),
                    tool_call_id: Some("call_0".into()),
                    name: Some("get_weather".into()),
                    tool_calls: None,
                },
            ],
            tools: vec![ToolDefinitionData {
                name: "get_weather".into(),
                description: Some("query weather".into()),
                parameters: Some(json!({"type": "object"})),
            }],
            tool_choice: Some(ToolChoice::Required),
            temperature: Some(0.2),
            top_p: None,
            max_output_tokens: Some(512),
            response_format: None,
            stream: false,
            metadata: Value::Null,
        }
    }

    fn provider() -> GeminiProvider {
        GeminiProvider::new(ProviderRuntimeConfig {
            provider_id: "p1".into(),
            provider_key: "gemini".into(),
            kind: ProviderKind::Gemini,
            base_url: "https://generativelanguage.googleapis.com".into(),
            credential: Some(SecretValue::new("g-key".to_string())),
            proxy_url: None,
            timeout_ms: 15000,
            max_retries: 1,
            config: Value::Null,
        })
    }

    #[test]
    fn request_body_maps_system_tools_history() {
        let (path, body) = provider().build_generate_body(&request_fixture(), false);
        assert_eq!(path, "/models/gemini-2.5-flash:generateContent");
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            json!("be brief")
        );
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[1]["role"], json!("model"));
        assert_eq!(
            contents[1]["parts"][0]["functionCall"]["name"],
            json!("get_weather")
        );
        assert_eq!(
            contents[2]["parts"][0]["functionResponse"]["name"],
            json!("get_weather")
        );
        assert_eq!(
            contents[2]["parts"][0]["functionResponse"]["response"],
            json!({"result": "sunny"})
        );
        assert_eq!(body["generationConfig"]["temperature"], json!(0.2));
        assert_eq!(body["generationConfig"]["maxOutputTokens"], json!(512));
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            json!("get_weather")
        );
        assert_eq!(
            body["toolConfig"]["functionCallingConfig"]["mode"],
            json!("ANY")
        );
    }

    #[test]
    fn stream_path_and_flag() {
        let (path, _) = provider().build_generate_body(&request_fixture(), true);
        assert_eq!(
            path,
            "/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn model_id_strips_prefix() {
        assert_eq!(
            GeminiProvider::model_id("models/gemini-2.0-flash"),
            "gemini-2.0-flash"
        );
    }

    #[test]
    fn response_maps_text_tools_usage() {
        let raw = json!({
            "modelVersion": "gemini-2.5-flash",
            "candidates": [{
                "finishReason": "STOP",
                "content": {"role": "model", "parts": [
                    {"text": "hello "},
                    {"text": "world"},
                    {"functionCall": {"name": "get_weather", "args": {"city": "北京"}}},
                ]},
            }],
            "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 3, "totalTokenCount": 10, "thoughtsTokenCount": 2},
        });
        let canonical = GeminiProvider::response_to_canonical(&raw);
        assert_eq!(canonical.content.as_deref(), Some("hello world"));
        assert_eq!(canonical.finish_reason.as_deref(), Some("stop"));
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].name, "get_weather");
        let usage = canonical.usage.unwrap();
        assert_eq!(usage.input_tokens, 7);
        assert_eq!(usage.reasoning_tokens, 2);
        assert_eq!(usage.total_tokens, 10);
    }

    #[test]
    fn thought_parts_become_reasoning() {
        let raw = json!({
            "candidates": [{
                "finishReason": "STOP",
                "content": {"parts": [
                    {"text": "thinking...", "thought": true},
                    {"text": "answer"},
                ]},
            }],
        });
        let canonical = GeminiProvider::response_to_canonical(&raw);
        assert_eq!(canonical.reasoning_content.as_deref(), Some("thinking..."));
        assert_eq!(canonical.content.as_deref(), Some("answer"));
    }
}
