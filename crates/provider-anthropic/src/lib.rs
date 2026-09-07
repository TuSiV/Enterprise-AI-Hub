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

//! Anthropic Messages API Provider Adapter（方案 §10：厂商协议只存在于 Adapter 内）。
//! 端点：POST {base}/v1/messages（非流式 / SSE 流式）、GET /v1/models。
//! 鉴权：x-api-key + anthropic-version 头。

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

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicFactory;

#[async_trait]
impl ProviderFactory for AnthropicFactory {
    fn protocol(&self) -> &'static str {
        "anthropic"
    }

    fn build(
        &self,
        config: ProviderRuntimeConfig,
    ) -> ProviderResult<std::sync::Arc<dyn aihub_provider_core::ModelProvider>> {
        Ok(std::sync::Arc::new(AnthropicProvider::new(config)))
    }
}

pub struct AnthropicProvider {
    http: reqwest::Client,
    base_url: String,
    credential: Option<SecretValue>,
    kind: ProviderKind,
    provider_key: String,
}

impl AnthropicProvider {
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

    /// base 兼容 `https://api.anthropic.com` 与 `.../v1` 两种填法。
    fn url(&self, path: &str) -> String {
        if self.base_url.ends_with("/v1") {
            format!("{}{path}", self.base_url)
        } else {
            format!("{}/v1{path}", self.base_url)
        }
    }

    fn auth_request(&self, method: reqwest::Method, url: String) -> reqwest::RequestBuilder {
        let mut rb = self.http.request(method, url).header("anthropic-version", ANTHROPIC_VERSION);
        if let Some(secret) = &self.credential {
            if !secret.is_empty() {
                rb = rb.header("x-api-key", secret.expose());
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
                let category = if message.to_lowercase().contains("token") {
                    ErrorCategory::ContextLengthExceeded
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

    /// Canonical → Messages API 请求体：system 抽取为顶层字段、
    /// tool 历史映射为 tool_use / tool_result 内容块、同角色消息合并。
    fn build_messages_body(&self, request: &CanonicalChatRequest, stream: bool) -> Value {
        let mut system_parts: Vec<String> = Vec::new();
        // Anthropic 要求 user/assistant 交替；同角色连续消息合并为一个多块消息。
        let mut messages: Vec<(String, Vec<Value>)> = Vec::new();
        for m in &request.messages {
            match m.role {
                MessageRole::System => system_parts.push(m.content.clone()),
                MessageRole::Tool => {
                    let block = json!({
                        "type": "tool_result",
                        "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                        "content": m.content.clone(),
                    });
                    push_block(&mut messages, "user", block);
                }
                MessageRole::Assistant => {
                    if !m.content.is_empty() {
                        push_block(&mut messages, "assistant", json!({
                            "type": "text",
                            "text": m.content.clone(),
                        }));
                    }
                    for call in m.tool_calls.iter().flatten() {
                        let input = serde_json::from_str::<Value>(&call.arguments)
                            .unwrap_or_else(|_| json!({}));
                        push_block(&mut messages, "assistant", json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.name,
                            "input": input,
                        }));
                    }
                }
                MessageRole::User => {
                    push_block(&mut messages, "user", json!({
                        "type": "text",
                        "text": m.content.clone(),
                    }));
                }
            }
        }

        let mut body = json!({
            "model": request.model,
            "max_tokens": request.max_output_tokens.unwrap_or(4096),
            "messages": messages.iter().map(|(role, blocks)| json!({
                "role": role,
                "content": blocks,
            })).collect::<Vec<_>>(),
        });
        if !system_parts.is_empty() {
            body["system"] = json!(system_parts.join("\n\n"));
        }
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    let mut tool = json!({
                        "name": t.name,
                        "input_schema": t.parameters.clone().unwrap_or(json!({"type": "object"})),
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
                    ToolChoice::Auto => json!({"type": "auto"}),
                    ToolChoice::Required => json!({"type": "any"}),
                    ToolChoice::Function(name) => json!({"type": "tool", "name": name}),
                    ToolChoice::None => {
                        body.as_object_mut().unwrap().remove("tools");
                        json!({"type": "auto"})
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
        if stream {
            body["stream"] = json!(true);
        }
        body
    }

    /// Messages API 响应体 → Canonical。
    fn response_to_canonical(resp: &Value) -> CanonicalChatResponse {
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();
        if let Some(blocks) = resp.get("content").and_then(|c| c.as_array()) {
            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            content.push_str(text);
                        }
                    }
                    Some("thinking") => {
                        if let Some(text) = block.get("thinking").and_then(|t| t.as_str()) {
                            reasoning.push_str(text);
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCallOutput {
                            id: block
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            arguments: block
                                .get("input")
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "{}".into()),
                        });
                    }
                    _ => {}
                }
            }
        }
        let usage = resp.get("usage").map(Self::map_usage);
        CanonicalChatResponse {
            content: if content.is_empty() { None } else { Some(content) },
            reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
            tool_calls,
            finish_reason: resp
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(map_stop_reason),
            usage,
            provider_model: resp
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }

    fn map_usage(usage: &Value) -> CanonicalUsage {
        let input = usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        let output = usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        CanonicalUsage {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            reasoning_tokens: 0,
            total_tokens: input + output,
            source: UsageSource::Provider,
        }
    }
}

fn push_block(messages: &mut Vec<(String, Vec<Value>)>, role: &str, block: Value) {
    if let Some(last) = messages.last_mut() {
        if last.0 == role {
            last.1.push(block);
            return;
        }
    }
    messages.push((role.to_string(), vec![block]));
}

fn map_stop_reason(reason: &str) -> String {
    match reason {
        "end_turn" | "stop_sequence" => "stop".into(),
        "tool_use" => "tool_calls".into(),
        "max_tokens" => "length".into(),
        "refusal" => "content_filter".into(),
        other => other.to_string(),
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
    }
    let truncated: String = body.chars().take(300).collect();
    if truncated.is_empty() {
        "provider returned an error".to_string()
    } else {
        truncated
    }
}

#[async_trait]
impl aihub_provider_core::ModelProvider for AnthropicProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    async fn list_models(&self) -> ProviderResult<Vec<DiscoveredModel>> {
        let mut models = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..10 {
            let mut url = format!("{}?limit=100", self.url("/models"));
            if let Some(cursor) = &after {
                url.push_str(&format!("&after_id={cursor}"));
            }
            let response = self
                .auth_request(reqwest::Method::GET, url)
                .send()
                .await
                .map_err(AnthropicProvider::transport_error)?;
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
            if let Some(data) = parsed.get("data").and_then(|d| d.as_array()) {
                for m in data {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    models.push(DiscoveredModel {
                        model_key: id.to_string(),
                        display_name: m
                            .get("display_name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        model_type: Some("chat".into()),
                        context_window: None,
                        capabilities: None,
                    });
                }
            }
            let has_more = parsed
                .get("has_more")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !has_more {
                break;
            }
            after = parsed
                .get("last_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        Ok(models)
    }

    async fn chat(&self, request: CanonicalChatRequest) -> ProviderResult<CanonicalChatResponse> {
        let body = self.build_messages_body(&request, false);
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/messages"))
            .json(&body)
            .send()
            .await
            .map_err(AnthropicProvider::transport_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(self.map_status_error(status, text).await);
        }
        let parsed: Value = response.json().await.map_err(|e| {
            ProviderError::new(
                ErrorCategory::MalformedResponse,
                format!("invalid messages response: {e}"),
            )
        })?;
        Ok(Self::response_to_canonical(&parsed))
    }

    async fn chat_stream(&self, request: CanonicalChatRequest) -> ProviderResult<ProviderStream> {
        let body = self.build_messages_body(&request, true);
        let response = self
            .auth_request(reqwest::Method::POST, self.url("/messages"))
            .json(&body)
            .send()
            .await
            .map_err(AnthropicProvider::transport_error)?;
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
            // tool_use 块的 index 在 content_block_start 给出，delta 阶段按 index 关联
            let mut current_block: Option<usize> = None;
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
                                    "message_start" if !started => {
                                        started = true;
                                        let model = event
                                            .pointer("/message/model")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                        if let Some(u) = event.pointer("/message/usage") {
                                            usage = Some(AnthropicProvider::map_usage(u));
                                        }
                                        yield StreamEvent::ResponseStarted { provider_model: model };
                                    }
                                    "content_block_start" => {
                                        let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        current_block = Some(index);
                                        if event.pointer("/content_block/type").and_then(|v| v.as_str()) == Some("tool_use") {
                                            yield StreamEvent::ToolCallStarted {
                                                index,
                                                id: event.pointer("/content_block/id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                                name: event.pointer("/content_block/name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                            };
                                        }
                                    }
                                    "content_block_stop" => {
                                        current_block = None;
                                    }
                                    "content_block_delta" => {
                                        let index = current_block.unwrap_or(0);
                                        let delta_type = event.pointer("/delta/type").and_then(|v| v.as_str()).unwrap_or("");
                                        match delta_type {
                                            "text_delta" => {
                                                if let Some(text) = event.pointer("/delta/text").and_then(|v| v.as_str()) {
                                                    if !text.is_empty() {
                                                        yield StreamEvent::ContentDelta { delta: text.to_string() };
                                                    }
                                                }
                                            }
                                            "thinking_delta" => {
                                                if let Some(text) = event.pointer("/delta/thinking").and_then(|v| v.as_str()) {
                                                    if !text.is_empty() {
                                                        yield StreamEvent::ReasoningDelta { delta: text.to_string() };
                                                    }
                                                }
                                            }
                                            "input_json_delta" => {
                                                if let Some(args) = event.pointer("/delta/partial_json").and_then(|v| v.as_str()) {
                                                    if !args.is_empty() {
                                                        yield StreamEvent::ToolCallArgumentsDelta { index, delta: args.to_string() };
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    "message_delta" => {
                                        if let Some(reason) = event.pointer("/delta/stop_reason").and_then(|v| v.as_str()) {
                                            finish_reason = Some(map_stop_reason(reason));
                                        }
                                        if let Some(u) = event.get("usage") {
                                            let mut canonical = AnthropicProvider::map_usage(u);
                                            if let Some(existing) = &usage {
                                                canonical.input_tokens = canonical.input_tokens.max(existing.input_tokens);
                                                canonical.output_tokens = canonical.output_tokens.max(existing.output_tokens);
                                                canonical.total_tokens = canonical.input_tokens + canonical.output_tokens;
                                            }
                                            usage = Some(canonical.clone());
                                            yield StreamEvent::UsageUpdated { usage: canonical };
                                        }
                                    }
                                    "error" => {
                                        let message = event
                                            .pointer("/error/message")
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
                        emitted_error = Some(AnthropicProvider::transport_error(e));
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
        _request: CanonicalEmbeddingRequest,
    ) -> ProviderResult<CanonicalEmbeddingResponse> {
        // Messages API 无 embeddings 端点；调用方应路由到 embedding 型 Provider。
        Err(ProviderError::new(
            ErrorCategory::InvalidRequest,
            format!("provider '{}' (anthropic) does not support embeddings", self.provider_key),
        ))
    }

    async fn health_check(&self) -> ProviderResult<ProviderHealth> {
        let started = Instant::now();
        let mut rb = self
            .http
            .request(reqwest::Method::GET, self.url("/models"))
            .timeout(Duration::from_secs(10))
            .header("anthropic-version", ANTHROPIC_VERSION);
        if let Some(secret) = &self.credential {
            if !secret.is_empty() {
                rb = rb.header("x-api-key", secret.expose());
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

#[cfg(test)]
mod tests {
    use super::*;
    use aihub_domain::canonical::CanonicalMessage;

    fn request_fixture() -> CanonicalChatRequest {
        CanonicalChatRequest {
            model: "claude-sonnet-4-5".into(),
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
                        id: "toolu_1".into(),
                        name: "get_weather".into(),
                        arguments: r#"{"city":"上海"}"#.into(),
                    }]),
                },
                CanonicalMessage {
                    role: MessageRole::Tool,
                    content: "sunny".into(),
                    tool_call_id: Some("toolu_1".into()),
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
            temperature: Some(0.5),
            top_p: None,
            max_output_tokens: Some(1024),
            response_format: None,
            stream: false,
            metadata: Value::Null,
        }
    }

    fn provider() -> AnthropicProvider {
        AnthropicProvider::new(ProviderRuntimeConfig {
            provider_id: "p1".into(),
            provider_key: "anthropic".into(),
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            credential: Some(SecretValue::new("sk-test".to_string())),
            proxy_url: None,
            timeout_ms: 15000,
            max_retries: 1,
            config: Value::Null,
        })
    }

    #[test]
    fn request_body_maps_system_tools_and_history() {
        let body = provider().build_messages_body(&request_fixture(), false);
        assert_eq!(body["system"], json!("be brief"));
        assert_eq!(body["max_tokens"], json!(1024));
        assert_eq!(body["temperature"], json!(0.5));
        let msgs = body["messages"].as_array().unwrap();
        // user / assistant(tool_use) / user(tool_result)
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], json!("user"));
        assert_eq!(msgs[1]["content"][0]["type"], json!("tool_use"));
        assert_eq!(msgs[1]["content"][0]["input"], json!({"city": "上海"}));
        assert_eq!(msgs[2]["content"][0]["type"], json!("tool_result"));
        assert_eq!(msgs[2]["content"][0]["tool_use_id"], json!("toolu_1"));
        assert_eq!(body["tools"][0]["name"], json!("get_weather"));
        assert_eq!(body["tool_choice"], json!({"type": "auto"}));
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn url_joins_v1_once() {
        assert_eq!(provider().url("/messages"), "https://api.anthropic.com/v1/messages");
        let mut with_v1 = provider();
        with_v1.base_url = "https://api.anthropic.com/v1".into();
        assert_eq!(with_v1.url("/messages"), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn response_maps_content_tools_usage() {
        let raw = json!({
            "model": "claude-sonnet-4-5",
            "stop_reason": "tool_use",
            "content": [
                {"type": "text", "text": "checking "},
                {"type": "tool_use", "id": "toolu_9", "name": "get_weather", "input": {"city": "北京"}},
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 3},
        });
        let canonical = AnthropicProvider::response_to_canonical(&raw);
        assert_eq!(canonical.content.as_deref(), Some("checking "));
        assert_eq!(canonical.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].arguments, r#"{"city":"北京"}"#);
        let usage = canonical.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.cached_input_tokens, 3);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn stop_reason_mapping() {
        assert_eq!(map_stop_reason("end_turn"), "stop");
        assert_eq!(map_stop_reason("max_tokens"), "length");
        assert_eq!(map_stop_reason("tool_use"), "tool_calls");
    }
}
