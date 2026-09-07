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

//! OpenAI-compatible Provider Adapter（方案附录 D：Generic OpenAI-compatible = Core Adapter）。
//! OpenAI / Ollama / 各类 openai_compatible 网关共用本 Adapter，通过 base_url 区分。

use std::time::{Duration, Instant};

use aihub_api_types::gateway as wire;
use aihub_domain::canonical::{
    CanonicalChatRequest, CanonicalChatResponse, CanonicalEmbeddingRequest,
    CanonicalEmbeddingResponse, CanonicalUsage, DiscoveredModel, StreamEvent, ToolCallOutput,
    ToolChoice, ToolDefinitionData, UsageSource,
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

pub struct OpenAICompatibleFactory;

#[async_trait]
impl ProviderFactory for OpenAICompatibleFactory {
    fn protocol(&self) -> &'static str {
        "openai_compatible"
    }

    fn build(
        &self,
        config: ProviderRuntimeConfig,
    ) -> ProviderResult<std::sync::Arc<dyn aihub_provider_core::ModelProvider>> {
        Ok(std::sync::Arc::new(OpenAICompatibleProvider::new(config)))
    }
}

pub struct OpenAICompatibleProvider {
    http: reqwest::Client,
    base_url: String,
    credential: Option<SecretValue>,
    extra_headers: Vec<(String, String)>,
    send_stream_options: bool,
    kind: ProviderKind,
    provider_key: String,
}

impl OpenAICompatibleProvider {
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
        let http = builder.build().unwrap_or_default();

        let mut extra_headers = Vec::new();
        if let Some(headers) = config
            .config
            .get("extraHeaders")
            .and_then(|v| v.as_object())
        {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    extra_headers.push((k.clone(), val.to_string()));
                }
            }
        }

        Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            credential: config.credential,
            extra_headers,
            send_stream_options: config
                .config
                .get("sendStreamOptions")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
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
        for (k, v) in &self.extra_headers {
            rb = rb.header(k, v);
        }
        rb
    }

    /// 统一错误映射（方案 §10.6）：基于 HTTP 状态与 OpenAI 错误体。
    async fn map_status_error(&self, status: u16, body: String) -> ProviderError {
        let message = extract_error_message(&body);
        let mut err = match status {
            401 => ProviderError::new(ErrorCategory::Authentication, message.clone()),
            403 => ProviderError::new(ErrorCategory::PermissionDenied, message.clone()),
            404 => ProviderError::new(ErrorCategory::ModelNotFound, message.clone()),
            429 => ProviderError::new(ErrorCategory::RateLimited, message.clone()),
            500..=599 => ProviderError::new(ErrorCategory::Provider5xx, message.clone()),
            _ => ProviderError::new(ErrorCategory::InvalidRequest, message.clone()),
        };
        err = err.with_status(status);
        if status == 429 {
            // OpenAI 风格 retry-after 头信息无法在此拿到 body 之外的信息，交给上层默认退避。
            err = err.with_retry_after(1000);
        }
        if matches!(status, 400) && message.to_lowercase().contains("context length") {
            err.category = ErrorCategory::ContextLengthExceeded;
        }
        if matches!(status, 400) && message.to_lowercase().contains("content filter") {
            err.category = ErrorCategory::ContentFiltered;
        }
        err
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

    fn map_usage(usage: Option<&wire::Usage>) -> Option<CanonicalUsage> {
        usage.map(|u| CanonicalUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cached_input_tokens: u
                .prompt_tokens_details
                .as_ref()
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
            reasoning_tokens: u
                .completion_tokens_details
                .as_ref()
                .map(|d| d.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: u.total_tokens.max(u.prompt_tokens + u.completion_tokens),
            source: UsageSource::Provider,
        })
    }

    fn build_chat_body(&self, request: &CanonicalChatRequest, stream: bool) -> Value {
        let mut body = json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                let mut obj = json!({"role": m.role.as_str(), "content": m.content});
                if let Some(id) = &m.tool_call_id {
                    obj["tool_call_id"] = json!(id);
                }
                if let Some(name) = &m.name {
                    obj["name"] = json!(name);
                }
                if let Some(calls) = &m.tool_calls {
                    obj["tool_calls"] = json!(calls.iter().map(|c| json!({
                        "id": c.id, "type": "function",
                        "function": {"name": c.name, "arguments": c.arguments}
                    })).collect::<Vec<_>>());
                }
                obj
            }).collect::<Vec<_>>(),
        });
        if !request.tools.is_empty() {
            body["tools"] = json!(request
                .tools
                .iter()
                .map(|t| json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters.clone().unwrap_or(json!({})),
                    }
                }))
                .collect::<Vec<_>>());
            if let Some(choice) = &request.tool_choice {
                body["tool_choice"] = match choice {
                    ToolChoice::Auto => json!("auto"),
                    ToolChoice::None => json!("none"),
                    ToolChoice::Required => json!("required"),
                    ToolChoice::Function(name) => {
                        json!({"type": "function", "function": {"name": name}})
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
            body["max_tokens"] = json!(max);
        }
        if let Some(rf) = &request.response_format {
            body["response_format"] = match rf {
                aihub_domain::canonical::ResponseFormat::Text => json!({"type": "text"}),
                aihub_domain::canonical::ResponseFormat::Json => json!({"type": "json_object"}),
            };
        }
        if stream {
            body["stream"] = json!(true);
            if self.send_stream_options {
                body["stream_options"] = json!({"include_usage": true});
            }
        }
        body
    }

    fn response_to_canonical(resp: wire::ChatCompletionResponse) -> CanonicalChatResponse {
        let choice = resp.choices.first();
        let message = choice.map(|c| &c.message);
        let tool_calls = message
            .and_then(|m| m.tool_calls.as_ref())
            .map(|calls| {
                calls
                    .iter()
                    .map(|c| ToolCallOutput {
                        id: c.id.clone().unwrap_or_default(),
                        name: c.function.name.clone().unwrap_or_default(),
                        arguments: c.function.arguments.clone().unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        CanonicalChatResponse {
            content: message
                .and_then(|m| m.content.as_ref())
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            reasoning_content: message.and_then(|m| m.reasoning_content.clone()),
            tool_calls,
            finish_reason: choice.and_then(|c| c.finish_reason.clone()),
            usage: Self::map_usage(resp.usage.as_ref()),
            provider_model: Some(resp.model),
        }
    }
}

fn extract_error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(message) = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
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
impl aihub_provider_core::ModelProvider for OpenAICompatibleProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    async fn list_models(&self) -> ProviderResult<Vec<DiscoveredModel>> {
        let response = self
            .auth_request(reqwest::Method::GET, self.url("/models"))
            .send()
            .await
            .map_err(Self::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, body).await);
        }
        let parsed: wire::ModelListResponse = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid /models response: {e}"),
            )
        })?;
        Ok(parsed
            .data
            .into_iter()
            .map(|m| {
                let key = m.id.clone();
                let model_type = if key.contains("embed") {
                    "embedding"
                } else if key.contains("rerank") {
                    "rerank"
                } else {
                    "chat"
                };
                DiscoveredModel {
                    model_key: m.id,
                    display_name: None,
                    model_type: Some(model_type.to_string()),
                    context_window: None,
                    capabilities: None,
                }
            })
            .collect())
    }

    async fn chat(&self, request: CanonicalChatRequest) -> ProviderResult<CanonicalChatResponse> {
        let body = self.build_chat_body(&request, false);
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/chat/completions"))
            .json(&body)
            .send()
            .await
            .map_err(Self::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: wire::ChatCompletionResponse = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid chat response: {e}"),
            )
        })?;
        Ok(Self::response_to_canonical(parsed))
    }

    async fn chat_stream(&self, request: CanonicalChatRequest) -> ProviderResult<ProviderStream> {
        let body = self.build_chat_body(&request, true);
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/chat/completions"))
            .json(&body)
            .send()
            .await
            .map_err(Self::transport_error)?;
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
                                if data == "[DONE]" {
                                    continue;
                                }
                                let Ok(chunk) = serde_json::from_str::<wire::ChatCompletionChunk>(data) else {
                                    continue;
                                };
                                if !started {
                                    started = true;
                                    yield StreamEvent::ResponseStarted { provider_model: Some(chunk.model.clone()) };
                                }
                                for choice in &chunk.choices {
                                    let delta = &choice.delta;
                                    if delta.reasoning_content.is_some() {
                                        yield StreamEvent::ReasoningDelta { delta: delta.reasoning_content.clone().unwrap_or_default() };
                                    }
                                    if let Some(content) = &delta.content {
                                        if !content.is_empty() {
                                            yield StreamEvent::ContentDelta { delta: content.clone() };
                                        }
                                    }
                                    if let Some(tool_calls) = &delta.tool_calls {
                                        for call in tool_calls {
                                            if let Some(id) = &call.id {
                                                yield StreamEvent::ToolCallStarted {
                                                    index: call.index,
                                                    id: id.clone(),
                                                    name: call.function.name.clone().unwrap_or_default(),
                                                };
                                            }
                                            if let Some(args) = &call.function.arguments {
                                                if !args.is_empty() {
                                                    yield StreamEvent::ToolCallArgumentsDelta { index: call.index, delta: args.clone() };
                                                }
                                            }
                                        }
                                    }
                                    if let Some(reason) = &choice.finish_reason {
                                        finish_reason = Some(reason.clone());
                                    }
                                }
                                if let Some(u) = &chunk.usage {
                                    let mut canonical = CanonicalUsage {
                                        input_tokens: u.prompt_tokens,
                                        output_tokens: u.completion_tokens,
                                        cached_input_tokens: u.prompt_tokens_details.as_ref().map(|d| d.cached_tokens).unwrap_or(0),
                                        reasoning_tokens: u.completion_tokens_details.as_ref().map(|d| d.reasoning_tokens).unwrap_or(0),
                                        total_tokens: u.total_tokens.max(u.prompt_tokens + u.completion_tokens),
                                        source: UsageSource::Provider,
                                    };
                                    if let Some(existing) = &usage {
                                        canonical.input_tokens = canonical.input_tokens.max(existing.input_tokens);
                                        canonical.output_tokens = canonical.output_tokens.max(existing.output_tokens);
                                    }
                                    usage = Some(canonical.clone());
                                    yield StreamEvent::UsageUpdated { usage: canonical };
                                }
                            }
                        }
                    }
                    Err(e) => {
                        emitted_error = Some(Self::transport_error(e));
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
            .map_err(Self::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: wire::EmbeddingResponse = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid embeddings response: {e}"),
            )
        })?;
        let mut embeddings = Vec::new();
        for item in &parsed.data {
            if let Some(vec) = item.embedding.as_array() {
                embeddings.push(
                    vec.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect(),
                );
            }
        }
        Ok(CanonicalEmbeddingResponse {
            embeddings,
            usage: Self::map_usage(parsed.usage.as_ref()),
        })
    }

    async fn health_check(&self) -> ProviderResult<ProviderHealth> {
        let started = Instant::now();
        let url = self.url("/models");
        let mut rb = self
            .http
            .request(reqwest::Method::GET, url)
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
impl OpenAICompatibleProvider {
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
}

/// Canonical 请求中 tools 字段的便捷访问。
pub fn tool_definitions(request: &CanonicalChatRequest) -> &[ToolDefinitionData] {
    &request.tools
}
