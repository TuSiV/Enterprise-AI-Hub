//! aihub-mock-openai：本地 OpenAI 兼容 mock 上游。
//! 用于开发/测试/冒烟：验证 Gateway 全链路而不依赖真实 Provider。
//! 故障注入：请求头 x-mock-behavior = 500 | 429 | 401 | timeout | slow

use aihub_api_types::gateway as wire;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

#[derive(Clone, Default)]
struct MockState {
    fail_next: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn echo_content(messages: &[wire::ChatMessage]) -> String {
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| match &m.content {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    format!("mock-echo: {last_user}")
}

fn usage(messages: &[wire::ChatMessage], output: &str) -> wire::Usage {
    let input_chars: usize = messages.iter().map(|m| m.content.to_string().len()).sum();
    wire::Usage {
        prompt_tokens: (input_chars / 4).max(1) as i64,
        completion_tokens: (output.len() / 4).max(1) as i64,
        total_tokens: 0,
        prompt_tokens_details: Some(wire::PromptTokensDetails { cached_tokens: 0 }),
        completion_tokens_details: Some(wire::CompletionTokensDetails { reasoning_tokens: 0 }),
    }
    .with_total()
}

trait UsageTotal {
    fn with_total(self) -> Self;
}

impl UsageTotal for wire::Usage {
    fn with_total(mut self) -> Self {
        self.total_tokens = self.prompt_tokens + self.completion_tokens;
        self
    }
}

async fn failure(headers: &HeaderMap, _state: &MockState) -> Option<Response> {
    let behavior = headers
        .get("x-mock-behavior")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    match behavior.as_deref() {
        Some("500") => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": {"message": "mock internal error", "type": "server_error"}})),
            )
                .into_response(),
        ),
        Some("429") => Some(
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({"error": {"message": "mock rate limited", "type": "rate_limit_error"}})),
            )
                .into_response(),
        ),
        Some("401") => Some(
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"message": "mock invalid api key", "type": "authentication_error"}})),
            )
                .into_response(),
        ),
        Some("timeout") => {
            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            Some(StatusCode::REQUEST_TIMEOUT.into_response())
        }
        Some("slow") => {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            None
        }
        _ => None,
    }
}

async fn models() -> Json<wire::ModelListResponse> {
    Json(wire::ModelListResponse {
        object: "list".into(),
        data: vec![
            wire::ModelInfo {
                id: "mock-mini".into(),
                object: "model".into(),
                created: unix_now(),
                owned_by: "mock".into(),
                metadata: None,
            },
            wire::ModelInfo {
                id: "mock-pro".into(),
                object: "model".into(),
                created: unix_now(),
                owned_by: "mock".into(),
                metadata: None,
            },
        ],
    })
}

async fn chat_completions(
    State(state): State<MockState>,
    headers: HeaderMap,
    Json(request): Json<wire::ChatCompletionRequest>,
) -> Response {
    if let Some(response) = failure(&headers, &state).await {
        return response;
    }
    let content = echo_content(&request.messages);
    let usage = usage(&request.messages, &content);

    if request.is_stream() {
        // SSE：按词分片输出
        let chunks = content.split_inclusive(' ').map(|s| s.to_string()).collect::<Vec<_>>();
        let usage_clone = usage.clone();
        let stream = async_stream::stream! {
            let id = format!("mock-{}", uuid::Uuid::new_v4());
            let model = request.model.clone();
            yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(
                serde_json::to_string(&wire::ChatCompletionChunk {
                    id: id.clone(),
                    object: "chat.completion.chunk".into(),
                    created: unix_now(),
                    model: model.clone(),
                    choices: vec![wire::ChunkChoice {
                        index: 0,
                        delta: wire::DeltaMessage { role: Some("assistant".into()), content: Some(String::new()), reasoning_content: None, tool_calls: None },
                        finish_reason: None,
                    }],
                    usage: None,
                }).unwrap_or_default(),
            ));
            for chunk in chunks {
                yield Ok(axum::response::sse::Event::default().data(
                    serde_json::to_string(&wire::ChatCompletionChunk {
                        id: id.clone(),
                        object: "chat.completion.chunk".into(),
                        created: unix_now(),
                        model: model.clone(),
                        choices: vec![wire::ChunkChoice {
                            index: 0,
                            delta: wire::DeltaMessage { role: None, content: Some(chunk), reasoning_content: None, tool_calls: None },
                            finish_reason: None,
                        }],
                        usage: None,
                    }).unwrap_or_default(),
                ));
            }
            yield Ok(axum::response::sse::Event::default().data(
                serde_json::to_string(&wire::ChatCompletionChunk {
                    id: id.clone(),
                    object: "chat.completion.chunk".into(),
                    created: unix_now(),
                    model: model.clone(),
                    choices: vec![wire::ChunkChoice {
                        index: 0,
                        delta: wire::DeltaMessage::default(),
                        finish_reason: Some("stop".into()),
                    }],
                    usage: None,
                }).unwrap_or_default(),
            ));
            yield Ok(axum::response::sse::Event::default().data(
                serde_json::to_string(&wire::ChatCompletionChunk {
                    id: id.clone(),
                    object: "chat.completion.chunk".into(),
                    created: unix_now(),
                    model: model.clone(),
                    choices: vec![],
                    usage: Some(usage_clone),
                }).unwrap_or_default(),
            ));
            yield Ok(axum::response::sse::Event::default().data("[DONE]"));
        };
        return axum::response::sse::Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response();
    }

    Json(wire::ChatCompletionResponse {
        id: format!("mock-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".into(),
        created: unix_now(),
        model: request.model.clone(),
        choices: vec![wire::Choice {
            index: 0,
            message: wire::ResponseMessage {
                role: "assistant".into(),
                content: Some(serde_json::Value::String(content.clone())),
                tool_calls: None,
                reasoning_content: None,
            },
            finish_reason: Some("stop".into()),
        }],
        usage: Some(usage),
    })
    .into_response()
}

async fn embeddings(Json(request): Json<wire::EmbeddingRequest>) -> Json<wire::EmbeddingResponse> {
    let inputs: Vec<serde_json::Value> = match &request.input {
        serde_json::Value::String(s) => vec![serde_json::Value::String(s.clone())],
        serde_json::Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    let data = inputs
        .into_iter()
        .enumerate()
        .map(|(index, _)| wire::EmbeddingData {
            object: "embedding".into(),
            embedding: serde_json::json!([0.1, 0.2, 0.3]),
            index,
        })
        .collect();
    Json(wire::EmbeddingResponse {
        object: "list".into(),
        data,
        model: request.model.clone(),
        usage: Some(wire::Usage {
            prompt_tokens: 10,
            completion_tokens: 0,
            total_tokens: 10,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }),
    })
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("MOCK_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9901);
    let state = MockState::default();
    let app = Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/embeddings", post(embeddings))
        .with_state(state);
    let addr = format!("127.0.0.1:{port}");
    println!("mock-openai listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
