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

//! Provider Adapter Contract Test（方案 §31.3 用例矩阵）：
//! 普通 Chat / Streaming（含 malformed SSE 容错）/ System Message / Usage 解析 /
//! 401 / 429 / 5xx / Timeout / Malformed Response / Context Too Long / Health。
//! 不通过本套件的 Adapter 不得标记为 Supported（§31.3）。

use aihub_domain::canonical::{CanonicalChatRequest, CanonicalMessage, MessageRole, StreamEvent};
use aihub_domain::entities::ProviderKind;
use aihub_domain::error::ErrorCategory;
use aihub_provider_core::{ModelProvider, ProviderFactory, ProviderRuntimeConfig};
use aihub_provider_openai_compatible::OpenAICompatibleFactory;
use aihub_secrets::SecretValue;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

async fn spawn_contract_upstream() -> String {
    async fn models() -> Json<Value> {
        Json(json!({"object": "list", "data": [{"id": "m1", "object": "model"}]}))
    }
    async fn chat(
        headers: axum::http::HeaderMap,
        Json(request): Json<Value>,
    ) -> axum::response::Response {
        let behavior = headers
            .get("x-contract-behavior")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("ok");

        // 流式用例（§31.3 Streaming / Malformed SSE 容错）
        if request["stream"].as_bool().unwrap_or(false) {
            let mut events: Vec<Result<axum::response::sse::Event, std::convert::Infallible>> = vec![
                Ok(axum::response::sse::Event::default().data(
                    json!({"id":"s1","object":"chat.completion.chunk","created":1,"model":"m1",
                           "choices":[{"index":0,"delta":{"role":"assistant","content":""}}]})
                    .to_string(),
                )),
                Ok(axum::response::sse::Event::default().data(
                    json!({"id":"s1","object":"chat.completion.chunk","created":1,"model":"m1",
                           "choices":[{"index":0,"delta":{"content":"hello"}}]})
                    .to_string(),
                )),
                Ok(axum::response::sse::Event::default().data(
                    json!({"id":"s1","object":"chat.completion.chunk","created":1,"model":"m1",
                           "choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
                           "usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}})
                    .to_string(),
                )),
                Ok(axum::response::sse::Event::default().data("[DONE]")),
            ];
            if behavior == "malformed_stream" {
                events.insert(
                    1,
                    Ok(axum::response::sse::Event::default().data("{bad json")),
                );
            }
            let stream = futures::stream::iter(events);
            return axum::response::sse::Sse::new(stream).into_response();
        }

        match behavior {
            "401" => {
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": {"message": "invalid api key", "type": "authentication_error"}
                    })),
                )
                    .into_response();
            }
            "429" => {
                return (
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({
                        "error": {"message": "rate limited", "type": "rate_limit_error"}
                    })),
                )
                    .into_response();
            }
            "500" => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": {"message": "boom", "type": "server_error"}
                    })),
                )
                    .into_response();
            }
            "malformed" => {
                return axum::response::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body("{not valid json".into())
                    .unwrap();
            }
            "context_too_long" => {
                return (axum::http::StatusCode::BAD_REQUEST, Json(json!({
                    "error": {"message": "This model's maximum context length is exceeded", "type": "invalid_request_error"}
                }))).into_response();
            }
            "timeout" => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                return axum::http::StatusCode::OK.into_response();
            }
            _ => {}
        }

        let has_system = request["messages"]
            .as_array()
            .map(|m| m.iter().any(|x| x["role"] == "system"))
            .unwrap_or(false);
        let content = if has_system { "system-ok" } else { "ok" };
        Json(json!({
            "id": "c1", "object": "chat.completion", "created": 1, "model": request["model"],
            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
            "usage": {
                "prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18,
                "prompt_tokens_details": {"cached_tokens": 3},
                "completion_tokens_details": {"reasoning_tokens": 2}
            }
        }))
        .into_response()
    }
    async fn models_route() -> Json<Value> {
        models().await
    }
    let app = Router::new()
        .route("/v1/models", get(models_route))
        .route("/v1/chat/completions", post(chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

fn provider(
    base_url: String,
    api_key: Option<&str>,
    behavior: Option<&str>,
    timeout_ms: u64,
) -> Arc<dyn ModelProvider> {
    let mut config = json!({});
    if let Some(behavior) = behavior {
        config["extraHeaders"] = json!({"x-contract-behavior": behavior});
    }
    OpenAICompatibleFactory
        .build(ProviderRuntimeConfig {
            provider_id: "contract".into(),
            provider_key: "contract".into(),
            kind: ProviderKind::OpenAICompatible,
            base_url,
            credential: api_key.map(SecretValue::new),
            proxy_url: None,
            timeout_ms: timeout_ms as i64,
            max_retries: 1,
            config,
        })
        .unwrap()
}

fn chat_request(stream: bool) -> CanonicalChatRequest {
    CanonicalChatRequest {
        model: "m1".into(),
        messages: vec![CanonicalMessage {
            role: MessageRole::User,
            content: "hello".into(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }],
        tools: vec![],
        tool_choice: None,
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        response_format: None,
        stream,
        metadata: json!({}),
    }
}

/// 普通 Chat + Usage 解析（§31.3 必测，含 cached/reasoning 细分）
#[tokio::test]
async fn contract_normal_chat_and_usage_parsing() {
    let url = spawn_contract_upstream().await;
    let response = provider(url.clone(), Some("sk-contract"), None, 10_000)
        .chat(chat_request(false))
        .await
        .expect("chat ok");
    assert_eq!(response.content.as_deref(), Some("ok"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    let usage = response.usage.expect("provider usage present");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.cached_input_tokens, 3);
    assert_eq!(usage.reasoning_tokens, 2);
    assert_eq!(usage.source, aihub_domain::canonical::UsageSource::Provider);
}

/// System Message 透传（§31.3 必测）
#[tokio::test]
async fn contract_system_message_forwarded() {
    let url = spawn_contract_upstream().await;
    let mut request = chat_request(false);
    request.messages.insert(
        0,
        CanonicalMessage {
            role: MessageRole::System,
            content: "你是契约测试系统提示".into(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
    );
    let response = provider(url, None, None, 10_000)
        .chat(request)
        .await
        .expect("chat ok");
    assert_eq!(response.content.as_deref(), Some("system-ok"));
}

/// Streaming + Usage 事件（§31.3 必测）
#[tokio::test]
async fn contract_streaming_with_usage_event() {
    let url = spawn_contract_upstream().await;
    let mut stream = provider(url, None, None, 10_000)
        .chat_stream(chat_request(true))
        .await
        .expect("stream opens");
    use futures::StreamExt;
    let mut content = String::new();
    let mut usage_seen = false;
    let mut completed = false;
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::ContentDelta { delta } => content.push_str(&delta),
            StreamEvent::UsageUpdated { usage } => {
                usage_seen = true;
                assert_eq!(usage.total_tokens, 7);
            }
            StreamEvent::ResponseCompleted { .. } => completed = true,
            _ => {}
        }
    }
    assert_eq!(content, "hello");
    assert!(usage_seen, "streaming usage event must surface");
    assert!(completed);
}

/// Malformed SSE 行容错：非法 JSON 被跳过，合法内容照常送达
#[tokio::test]
async fn contract_streaming_tolerates_malformed_lines() {
    let url = spawn_contract_upstream().await;
    let mut stream = provider(url, None, Some("malformed_stream"), 10_000)
        .chat_stream(chat_request(true))
        .await
        .expect("stream opens");
    use futures::StreamExt;
    let mut content = String::new();
    while let Some(event) = stream.next().await {
        if let StreamEvent::ContentDelta { delta } = event {
            content.push_str(&delta);
        }
    }
    assert_eq!(content, "hello");
}

/// 401 → Authentication（§31.3 必测）
#[tokio::test]
async fn contract_401_maps_to_authentication() {
    let url = spawn_contract_upstream().await;
    let err = provider(url, Some("wrong"), Some("401"), 10_000)
        .chat(chat_request(false))
        .await
        .expect_err("401 must error");
    assert_eq!(err.category, ErrorCategory::Authentication);
    assert_eq!(err.http_status, Some(401));
    assert!(!err.category.retryable());
}

/// 429 → RateLimited 且可重试（§31.3 必测）
#[tokio::test]
async fn contract_429_maps_to_rate_limited() {
    let url = spawn_contract_upstream().await;
    let err = provider(url, None, Some("429"), 10_000)
        .chat(chat_request(false))
        .await
        .expect_err("429 must error");
    assert_eq!(err.category, ErrorCategory::RateLimited);
    assert!(err.category.retryable());
}

/// 5xx → Provider5xx 且可重试（§31.3 必测）
#[tokio::test]
async fn contract_5xx_maps_to_provider_5xx() {
    let url = spawn_contract_upstream().await;
    let err = provider(url, None, Some("500"), 10_000)
        .chat(chat_request(false))
        .await
        .expect_err("5xx must error");
    assert_eq!(err.category, ErrorCategory::Provider5xx);
    assert!(err.category.retryable());
}

/// Context Too Long（§31.3 必测）
#[tokio::test]
async fn contract_context_too_long_maps() {
    let url = spawn_contract_upstream().await;
    let err = provider(url, None, Some("context_too_long"), 10_000)
        .chat(chat_request(false))
        .await
        .expect_err("must error");
    assert_eq!(err.category, ErrorCategory::ContextLengthExceeded);
    assert!(!err.category.retryable());
}

/// Malformed Response（§31.3 必测）
#[tokio::test]
async fn contract_malformed_response_maps() {
    let url = spawn_contract_upstream().await;
    let err = provider(url, None, Some("malformed"), 10_000)
        .chat(chat_request(false))
        .await
        .expect_err("must error");
    assert_eq!(err.category, ErrorCategory::MalformedResponse);
}

/// Timeout（§31.3 必测）
#[tokio::test]
async fn contract_timeout_maps() {
    let url = spawn_contract_upstream().await;
    let err = provider(url, None, Some("timeout"), 500)
        .chat(chat_request(false))
        .await
        .expect_err("must error");
    assert_eq!(err.category, ErrorCategory::Timeout);
    assert!(err.category.retryable());
}

/// Health Check
#[tokio::test]
async fn contract_health_check() {
    let url = spawn_contract_upstream().await;
    let health = provider(url, None, None, 10_000)
        .health_check()
        .await
        .expect("health ok");
    assert_eq!(health.status, "healthy");
    assert!(health.latency_ms.is_some());
}
