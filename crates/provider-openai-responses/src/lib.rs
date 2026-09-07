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

//! OpenAI Responses API Provider Adapter（POST {base}/responses）。
//! 由 Provider config `{"api": "responses"}` 选择（openai / openai_compatible 均可），
//! 适用于 o 系列 / gpt-5 等以 Responses 为主接口的上游。

use std::collections::HashMap;
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

pub struct OpenAIResponsesFactory;

#[async_trait]
impl ProviderFactory for OpenAIResponsesFactory {
    fn protocol(&self) -> &'static str {
        "openai_responses"
    }

    fn build(
        &self,
        config: ProviderRuntimeConfig,
    ) -> ProviderResult<std::sync::Arc<dyn aihub_provider_core::ModelProvider>> {
        Ok(std::sync::Arc::new(OpenAIResponsesProvider::new(config)))
    }
}

pub struct OpenAIResponsesProvider {
    http: reqwest::Client,
    base_url: String,
    credential: Option<SecretValue>,
    kind: ProviderKind,
    provider_key: String,
}

impl OpenAIResponsesProvider {
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

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn auth_request(&self, method: reqwest::Method, url: String) -> reqwest::RequestBuilder {
        let mut rb = self.http.request(method, url);
        if let Some(secret) = &self.credential {
            if !secret.is_empty() {
                rb = rb.bearer_auth(secret.expose());
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
            401 => ProviderError::new(ErrorCategory::Authentication, message.clone()),
            403 => ProviderError::new(ErrorCategory::PermissionDenied, message.clone()),
            404 => ProviderError::new(ErrorCategory::ModelNotFound, message.clone()),
            429 => ProviderError::new(ErrorCategory::RateLimited, message.clone()),
            500..=599 => ProviderError::new(ErrorCategory::Provider5xx, message.clone()),
            _ => {
                let lower = message.to_lowercase();
                let category = if lower.contains("context length") || lower.contains("too long") {
                    ErrorCategory::ContextLengthExceeded
                } else if lower.contains("content filter") || lower.contains("content_policy") {
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

    /// Canonical → Responses API 请求体。
    fn build_responses_body(&self, request: &CanonicalChatRequest, stream: bool) -> Value {
        let mut input: Vec<Value> = Vec::new();
        for m in &request.messages {
            match m.role {
                MessageRole::System => {
                    input.push(json!({"role": "system", "content": m.content}));
                }
                MessageRole::User => {
                    input.push(json!({
                        "role": "user",
                        "content": [{"type": "input_text", "text": m.content}],
                    }));
                }
                MessageRole::Assistant => {
                    if !m.content.is_empty() {
                        input.push(json!({
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": m.content}],
                        }));
                    }
                    for call in m.tool_calls.iter().flatten() {
                        input.push(json!({
                            "type": "function_call",
                            "call_id": call.id,
                            "name": call.name,
                            "arguments": call.arguments,
                        }));
                    }
                }
                MessageRole::Tool => {
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": m.tool_call_id.clone().unwrap_or_default(),
                        "output": m.content,
                    }));
                }
            }
        }

        let mut body = json!({"model": request.model, "input": input});
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    let mut tool = json!({
                        "type": "function",
                        "name": t.name,
                        "parameters": t.parameters.clone().unwrap_or(json!({"type": "object"})),
                    });
                    if let Some(desc) = &t.description {
                        tool["description"] = json!(desc);
                    }
                    tool
                })
                .collect();
            body["tools"] = json!(tools);
            if let Some(choice) = &request.tool_choice {
                body["tool_choice"] = match choice {
                    ToolChoice::Auto => json!("auto"),
                    ToolChoice::None => json!("none"),
                    ToolChoice::Required => json!("required"),
                    ToolChoice::Function(name) => {
                        json!({"type": "function", "name": name})
                    }
                };
            }
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(p) = request.top_p {
            body["top_p"] = json!(p);
        }
        if let Some(max) = request.max_output_tokens {
            body["max_output_tokens"] = json!(max);
        }
        if let Some(rf) = &request.response_format {
            body["text"] = json!({"format": match rf {
                aihub_domain::canonical::ResponseFormat::Text => json!({"type": "text"}),
                aihub_domain::canonical::ResponseFormat::Json => json!({"type": "json_object"}),
            }});
        }
        if stream {
            body["stream"] = json!(true);
        }
        body
    }

    /// Responses API 响应 → Canonical。
    fn response_to_canonical(resp: &Value) -> CanonicalChatResponse {
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();

        if let Some(items) = resp.get("output").and_then(|o| o.as_array()) {
            for item in items {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("message") => {
                        if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                            for part in parts {
                                if part.get("type").and_then(|t| t.as_str()) == Some("output_text")
                                {
                                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                        content.push_str(text);
                                    }
                                }
                            }
                        }
                    }
                    Some("reasoning") => {
                        if let Some(parts) = item.get("summary").and_then(|s| s.as_array()) {
                            for part in parts {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    reasoning.push_str(text);
                                }
                            }
                        }
                    }
                    Some("function_call") => {
                        tool_calls.push(ToolCallOutput {
                            id: item
                                .get("call_id")
                                .or_else(|| item.get("id"))
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            name: item
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            arguments: item
                                .get("arguments")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }

        let finish_reason = if !tool_calls.is_empty() {
            Some("tool_calls".to_string())
        } else {
            resp.get("status")
                .and_then(|s| s.as_str())
                .and_then(|s| match s {
                    "completed" => Some("stop"),
                    "incomplete" => Some("length"),
                    _ => None,
                })
                .map(|s| s.to_string())
        };

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
            usage: resp.get("usage").map(Self::map_usage),
            provider_model: resp
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }

    fn map_usage(usage: &Value) -> CanonicalUsage {
        let input = usage
            .get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        CanonicalUsage {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            reasoning_tokens: usage
                .pointer("/output_tokens_details/reasoning_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            total_tokens: usage
                .get("total_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(input + output),
            source: UsageSource::Provider,
        }
    }
}

fn extract_error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(message) = value.pointer("/error/message").and_then(|m| m.as_str()) {
            return message.to_string();
        }
        if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
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
impl aihub_provider_core::ModelProvider for OpenAIResponsesProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    async fn list_models(&self) -> ProviderResult<Vec<DiscoveredModel>> {
        let response = self
            .auth_request(reqwest::Method::GET, self.url("/models"))
            .send()
            .await
            .map_err(OpenAIResponsesProvider::transport_error)?;
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
        Ok(parsed
            .get("data")
            .and_then(|d| d.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?;
                        let model_type = if id.contains("embed") {
                            "embedding"
                        } else if id.contains("rerank") {
                            "rerank"
                        } else {
                            "chat"
                        };
                        Some(DiscoveredModel {
                            model_key: id.to_string(),
                            display_name: None,
                            model_type: Some(model_type.to_string()),
                            context_window: None,
                            capabilities: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn chat(&self, request: CanonicalChatRequest) -> ProviderResult<CanonicalChatResponse> {
        let body = self.build_responses_body(&request, false);
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/responses"))
            .json(&body)
            .send()
            .await
            .map_err(OpenAIResponsesProvider::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: Value = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid responses body: {e}"),
            )
        })?;
        Ok(Self::response_to_canonical(&parsed))
    }

    async fn chat_stream(&self, request: CanonicalChatRequest) -> ProviderResult<ProviderStream> {
        let body = self.build_responses_body(&request, true);
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/responses"))
            .json(&body)
            .send()
            .await
            .map_err(OpenAIResponsesProvider::transport_error)?;
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
            let mut saw_function_call = false;
            // output_index → tool 序号（Responses 的 delta 只带 output_index/item_id）
            let mut tool_indexes: HashMap<String, usize> = HashMap::new();
            let mut next_tool_index: usize = 0;
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
                                let Ok(event) = serde_json::from_str::<Value>(data) else { continue };
                                let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                match event_type {
                                    "response.created" if !started => {
                                        started = true;
                                        let model = event.pointer("/response/model")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                        yield StreamEvent::ResponseStarted { provider_model: model };
                                    }
                                    "response.output_item.added"
                                        if event.pointer("/item/type").and_then(|v| v.as_str()) == Some("function_call") =>
                                    {
                                        saw_function_call = true;
                                            let output_index = event.get("output_index")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0)
                                                .to_string();
                                            let tool_index = next_tool_index;
                                            next_tool_index += 1;
                                            tool_indexes.insert(output_index, tool_index);
                                            let id = event.pointer("/item/call_id")
                                                .or_else(|| event.pointer("/item/id"))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string();
                                            let name = event.pointer("/item/name")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or_default()
                                                .to_string();
                                            yield StreamEvent::ToolCallStarted { index: tool_index, id, name };
                                    }
                                    "response.output_text.delta" => {
                                        if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                                            if !delta.is_empty() {
                                                yield StreamEvent::ContentDelta { delta: delta.to_string() };
                                            }
                                        }
                                    }
                                    "response.reasoning_summary_text.delta" => {
                                        if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                                            if !delta.is_empty() {
                                                yield StreamEvent::ReasoningDelta { delta: delta.to_string() };
                                            }
                                        }
                                    }
                                    "response.function_call_arguments.delta" => {
                                        let output_index = event.get("output_index")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            .to_string();
                                        let tool_index = tool_indexes.get(&output_index).copied().unwrap_or(0);
                                        if let Some(delta) = event
                                            .get("delta")
                                            .and_then(|v| v.as_str())
                                            .filter(|d| !d.is_empty())
                                        {
                                            yield StreamEvent::ToolCallArgumentsDelta { index: tool_index, delta: delta.to_string() };
                                        }
                                    }
                                    "response.completed" => {
                                        if let Some(u) = event.pointer("/response/usage") {
                                            let canonical = OpenAIResponsesProvider::map_usage(u);
                                            usage = Some(canonical.clone());
                                            yield StreamEvent::UsageUpdated { usage: canonical };
                                        }
                                        finish_reason = Some(if saw_function_call {
                                            "tool_calls".to_string()
                                        } else {
                                            event.pointer("/response/status")
                                                .and_then(|v| v.as_str())
                                                .and_then(|s| match s {
                                                    "completed" => Some("stop"),
                                                    "incomplete" => Some("length"),
                                                    _ => None,
                                                })
                                                .unwrap_or("stop")
                                                .to_string()
                                        });
                                    }
                                    "response.failed" | "error" => {
                                        let message = event
                                            .pointer("/response/error/message")
                                            .or_else(|| event.pointer("/error/message"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("provider stream error")
                                            .to_string();
                                        yield StreamEvent::ProviderError {
                                            category: ErrorCategory::Provider5xx,
                                            message,
                                            http_status: None,
                                        };
                                        return;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Err(e) => {
                        emitted_error = Some(OpenAIResponsesProvider::transport_error(e));
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
        let body = json!({"model": request.model, "input": request.inputs});
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/embeddings"))
            .json(&body)
            .send()
            .await
            .map_err(OpenAIResponsesProvider::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: Value = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid embeddings response: {e}"),
            )
        })?;
        let embeddings = parsed
            .get("data")
            .and_then(|d| d.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        item.get("embedding")
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
        let usage = parsed.get("usage").map(|u| {
            let input = u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let output = u
                .get("total_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(input);
            CanonicalUsage {
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
                total_tokens: output,
                source: UsageSource::Provider,
            }
        });
        Ok(CanonicalEmbeddingResponse { embeddings, usage })
    }

    async fn health_check(&self) -> ProviderResult<ProviderHealth> {
        let started = Instant::now();
        let mut rb = self
            .http
            .request(reqwest::Method::GET, self.url("/models"))
            .timeout(Duration::from_secs(10));
        if let Some(secret) = &self.credential {
            if !secret.is_empty() {
                rb = rb.bearer_auth(secret.expose());
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
                            401 => ErrorCategory::Authentication,
                            403 => ErrorCategory::PermissionDenied,
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
impl OpenAIResponsesProvider {
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aihub_domain::canonical::CanonicalMessage;

    fn request_fixture() -> CanonicalChatRequest {
        CanonicalChatRequest {
            model: "gpt-5.1".into(),
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
                        id: "call_abc".into(),
                        name: "get_weather".into(),
                        arguments: r#"{"city":"上海"}"#.into(),
                    }]),
                },
                CanonicalMessage {
                    role: MessageRole::Tool,
                    content: "sunny".into(),
                    tool_call_id: Some("call_abc".into()),
                    name: None,
                    tool_calls: None,
                },
            ],
            tools: vec![aihub_domain::canonical::ToolDefinitionData {
                name: "get_weather".into(),
                description: Some("query weather".into()),
                parameters: Some(json!({"type": "object"})),
            }],
            tool_choice: Some(ToolChoice::Auto),
            temperature: None,
            top_p: None,
            max_output_tokens: Some(2048),
            response_format: None,
            stream: false,
            metadata: Value::Null,
        }
    }

    fn provider() -> OpenAIResponsesProvider {
        OpenAIResponsesProvider::new(ProviderRuntimeConfig {
            provider_id: "p1".into(),
            provider_key: "openai-responses".into(),
            kind: ProviderKind::OpenAI,
            base_url: "https://api.openai.com/v1".into(),
            credential: Some(SecretValue::new("sk-test".to_string())),
            proxy_url: None,
            timeout_ms: 15000,
            max_retries: 1,
            config: Value::Null,
        })
    }

    #[test]
    fn request_body_maps_input_and_tools() {
        let body = provider().build_responses_body(&request_fixture(), false);
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["role"], json!("system"));
        assert_eq!(input[1]["content"][0]["type"], json!("input_text"));
        assert_eq!(input[2]["type"], json!("function_call"));
        assert_eq!(input[2]["call_id"], json!("call_abc"));
        assert_eq!(input[3]["type"], json!("function_call_output"));
        assert_eq!(body["tools"][0]["name"], json!("get_weather"));
        assert_eq!(body["tool_choice"], json!("auto"));
        assert_eq!(body["max_output_tokens"], json!(2048));
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn response_maps_output_items_and_usage() {
        let raw = json!({
            "id": "resp_1",
            "model": "gpt-5.1",
            "status": "completed",
            "output": [
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "hmm"}]},
                {"type": "message", "role": "assistant", "content": [
                    {"type": "output_text", "text": "hello"},
                ]},
                {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"北京\"}"},
            ],
            "usage": {
                "input_tokens": 12,
                "output_tokens": 8,
                "total_tokens": 20,
                "input_tokens_details": {"cached_tokens": 4},
                "output_tokens_details": {"reasoning_tokens": 6},
            },
        });
        let canonical = OpenAIResponsesProvider::response_to_canonical(&raw);
        assert_eq!(canonical.content.as_deref(), Some("hello"));
        assert_eq!(canonical.reasoning_content.as_deref(), Some("hmm"));
        assert_eq!(canonical.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].id, "call_1");
        let usage = canonical.usage.unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.cached_input_tokens, 4);
        assert_eq!(usage.reasoning_tokens, 6);
        assert_eq!(usage.total_tokens, 20);
    }

    #[test]
    fn completed_without_tools_maps_stop() {
        let raw = json!({
            "model": "gpt-5.1",
            "status": "completed",
            "output": [
                {"type": "message", "content": [{"type": "output_text", "text": "done"}]},
            ],
            "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
        });
        let canonical = OpenAIResponsesProvider::response_to_canonical(&raw);
        assert_eq!(canonical.finish_reason.as_deref(), Some("stop"));
    }
}
