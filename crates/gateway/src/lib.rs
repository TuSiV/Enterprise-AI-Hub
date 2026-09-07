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

//! OpenAI-Compatible Gateway HTTP 层（方案 §11）：
//! GET /v1/models, POST /v1/chat/completions（流式/非流式）, POST /v1/embeddings。
//! 本 crate 只做协议适配；业务在 application::pipeline。

mod embeddings;
mod models_list;

use aihub_api_types::gateway as wire;
use aihub_application::error::PipelineError;
use aihub_application::pipeline::{ChatPipeline, PipelineStreamEvent, StreamExecution};
use aihub_application::resolver::ModelResolver;
use aihub_application::Repos;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use std::sync::Arc;

pub use embeddings::execute_embeddings;

#[derive(Clone)]
pub struct GatewayState {
    pub pipeline: Arc<ChatPipeline>,
    pub resolver: Arc<ModelResolver>,
    pub registry: Arc<aihub_application::registry::ProviderRegistry>,
    pub repos: Repos,
}

pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/embeddings", post(embeddings_handler))
        .with_state(state)
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Gateway 错误以 OpenAI 错误体返回，同时携带 AIH 错误码（§11/§27.2）。
pub fn pipeline_error_response(err: &PipelineError) -> Response {
    let status = StatusCode::from_u16(err.code.http_status()).unwrap_or(StatusCode::BAD_GATEWAY);
    let body = wire::GatewayError::new(err.message.clone(), "aihub_error", err.code.as_str());
    (status, Json(body)).into_response()
}

/// 本地错误包装：实现 IntoResponse（孤儿规则要求类型定义在本 crate）。
struct GatewayError(PipelineError);

impl axum::response::IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        pipeline_error_response(&self.0)
    }
}

async fn authenticate(
    state: &GatewayState,
    headers: &HeaderMap,
) -> Result<aihub_application::pipeline::AuthContext, GatewayError> {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());
    let Some(bearer) = bearer else {
        return Err(GatewayError(PipelineError::new(
            aihub_application::error::GatewayCode::Unauthorized,
            "missing Authorization: Bearer <application api key>",
        )));
    };
    state
        .pipeline
        .authenticate_key(&bearer)
        .await
        .map_err(GatewayError)
}

/// 扩展响应头（§11.4）
fn aih_headers(
    response: &mut Response,
    request_id: &str,
    resolved_model: &str,
    provider: &str,
    retries: i32,
) {
    let headers = response.headers_mut();
    if let Ok(v) = axum::http::HeaderValue::from_str(request_id) {
        headers.insert("x-aih-request-id", v);
    }
    if let Ok(v) = axum::http::HeaderValue::from_str(resolved_model) {
        headers.insert("x-aih-resolved-model", v);
    }
    if let Ok(v) = axum::http::HeaderValue::from_str(provider) {
        headers.insert("x-aih-provider", v);
    }
    if let Ok(v) = axum::http::HeaderValue::from_str(&retries.to_string()) {
        headers.insert("x-aih-retry-count", v);
    }
}

async fn list_models(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Response, GatewayError> {
    let ctx = authenticate(&state, &headers).await?;
    let response = models_list::list_models(&state, &ctx)
        .await
        .map_err(GatewayError)?;
    Ok(Json(response).into_response())
}

async fn chat_completions(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<wire::ChatCompletionRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(e) => return e.into_response(),
    };
    let stream = request.is_stream();
    let model_name = request.model.clone();
    let canonical = to_canonical(&request, stream);

    if !stream {
        return chat_non_stream(&state, ctx, canonical).await;
    }
    chat_stream(&state, ctx, canonical, model_name).await
}

fn to_canonical(
    request: &wire::ChatCompletionRequest,
    stream: bool,
) -> aihub_domain::canonical::CanonicalChatRequest {
    use aihub_domain::canonical::*;
    let messages = request
        .messages
        .iter()
        .map(|m| CanonicalMessage {
            role: match m.role.as_str() {
                "system" => MessageRole::System,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::Tool,
                _ => MessageRole::User,
            },
            content: wire_content_to_text(&m.content),
            tool_call_id: m.tool_call_id.clone(),
            name: m.name.clone(),
            tool_calls: m.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|c| ToolCallOutput {
                        id: c.id.clone().unwrap_or_default(),
                        name: c.function.name.clone().unwrap_or_default(),
                        arguments: c.function.arguments.clone().unwrap_or_default(),
                    })
                    .collect()
            }),
        })
        .collect();
    let tools = request
        .tools
        .iter()
        .flatten()
        .map(|t| ToolDefinitionData {
            name: t.function.name.clone(),
            description: t.function.description.clone(),
            parameters: t.function.parameters.clone(),
        })
        .collect();
    CanonicalChatRequest {
        model: request.model.clone(),
        messages,
        tools,
        tool_choice: None,
        temperature: request.temperature,
        top_p: request.top_p,
        max_output_tokens: request.max_output_tokens(),
        response_format: None,
        stream,
        metadata: serde_json::json!({
            "streamOptionsIncludeUsage": request.stream_options.as_ref().and_then(|s| s.include_usage).unwrap_or(false),
        }),
    }
}

fn wire_content_to_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

async fn chat_non_stream(
    state: &GatewayState,
    ctx: aihub_application::pipeline::AuthContext,
    canonical: aihub_domain::canonical::CanonicalChatRequest,
) -> Response {
    match state.pipeline.execute_chat(&ctx, canonical).await {
        Ok(execution) => {
            let response = to_wire_response(&execution);
            let mut resp = Json(response).into_response();
            aih_headers(
                &mut resp,
                &execution.request_id,
                &execution.resolved_model_key,
                &execution.provider_key,
                execution.retry_count,
            );
            resp
        }
        Err(e) => pipeline_error_response(&e),
    }
}

fn to_wire_response(
    execution: &aihub_application::pipeline::ChatExecution,
) -> wire::ChatCompletionResponse {
    let response = &execution.response;
    let message = wire::ResponseMessage {
        role: "assistant".to_string(),
        content: response.content.clone().map(serde_json::Value::String),
        tool_calls: None,
        reasoning_content: response.reasoning_content.clone(),
    };
    let usage = execution.usage_cost.as_ref().map(|uc| wire::Usage {
        prompt_tokens: uc.usage.input_tokens,
        completion_tokens: uc.usage.output_tokens,
        total_tokens: uc.usage.total_tokens,
        prompt_tokens_details: Some(wire::PromptTokensDetails {
            cached_tokens: uc.usage.cached_input_tokens,
        }),
        completion_tokens_details: Some(wire::CompletionTokensDetails {
            reasoning_tokens: uc.usage.reasoning_tokens,
        }),
    });
    wire::ChatCompletionResponse {
        id: execution.request_id.clone(),
        object: "chat.completion".to_string(),
        created: unix_now(),
        model: execution
            .virtual_model_key
            .clone()
            .unwrap_or_else(|| execution.resolved_model_key.clone()),
        choices: vec![wire::Choice {
            index: 0,
            message,
            finish_reason: response
                .finish_reason
                .clone()
                .or_else(|| Some("stop".to_string())),
        }],
        usage,
    }
}

async fn chat_stream(
    state: &GatewayState,
    ctx: aihub_application::pipeline::AuthContext,
    canonical: aihub_domain::canonical::CanonicalChatRequest,
    model: String,
) -> Response {
    let StreamExecution {
        request_id,
        trace_id: _,
        mut rx,
    } = match state.pipeline.execute_chat_stream(&ctx, canonical).await {
        Ok(execution) => execution,
        Err(e) => return pipeline_error_response(&e),
    };
    let id = request_id.clone();

    let event_stream = async_stream::stream! {
        // 首个 chunk 携带 assistant role（OpenAI 习惯）
        yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(
            serde_json::to_string(&wire::ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk".into(),
                created: unix_now(),
                model: model.clone(),
                choices: vec![wire::ChunkChoice {
                    index: 0,
                    delta: wire::DeltaMessage {
                        role: Some("assistant".into()),
                        content: Some(String::new()),
                        reasoning_content: None,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
                usage: None,
            })
            .unwrap_or_default(),
        ));

        while let Some(event) = rx.recv().await {
            match event {
                PipelineStreamEvent::Content { delta } => {
                    yield Ok(axum::response::sse::Event::default().data(
                        serde_json::to_string(&wire::ChatCompletionChunk {
                            id: id.clone(),
                            object: "chat.completion.chunk".into(),
                            created: unix_now(),
                            model: model.clone(),
                            choices: vec![wire::ChunkChoice {
                                index: 0,
                                delta: wire::DeltaMessage {
                                    role: None,
                                    content: Some(delta),
                                    reasoning_content: None,
                                    tool_calls: None,
                                },
                                finish_reason: None,
                            }],
                            usage: None,
                        })
                        .unwrap_or_default(),
                    ));
                }
                PipelineStreamEvent::Reasoning { delta } => {
                    yield Ok(axum::response::sse::Event::default().data(
                        serde_json::to_string(&wire::ChatCompletionChunk {
                            id: id.clone(),
                            object: "chat.completion.chunk".into(),
                            created: unix_now(),
                            model: model.clone(),
                            choices: vec![wire::ChunkChoice {
                                index: 0,
                                delta: wire::DeltaMessage {
                                    role: None,
                                    content: None,
                                    reasoning_content: Some(delta),
                                    tool_calls: None,
                                },
                                finish_reason: None,
                            }],
                            usage: None,
                        })
                        .unwrap_or_default(),
                    ));
                }
                PipelineStreamEvent::ToolCallStarted { index, id: call_id, name } => {
                    yield Ok(axum::response::sse::Event::default().data(
                        serde_json::to_string(&wire::ChatCompletionChunk {
                            id: id.clone(),
                            object: "chat.completion.chunk".into(),
                            created: unix_now(),
                            model: model.clone(),
                            choices: vec![wire::ChunkChoice {
                                index: 0,
                                delta: wire::DeltaMessage {
                                    role: None,
                                    content: None,
                                    reasoning_content: None,
                                    tool_calls: Some(vec![wire::ToolCall {
                                        index,
                                        id: Some(call_id),
                                        call_type: "function".into(),
                                        function: wire::ToolCallFunction {
                                            name: Some(name),
                                            arguments: Some(String::new()),
                                        },
                                    }]),
                                },
                                finish_reason: None,
                            }],
                            usage: None,
                        })
                        .unwrap_or_default(),
                    ));
                }
                PipelineStreamEvent::ToolCallArguments { index, delta } => {
                    yield Ok(axum::response::sse::Event::default().data(
                        serde_json::to_string(&wire::ChatCompletionChunk {
                            id: id.clone(),
                            object: "chat.completion.chunk".into(),
                            created: unix_now(),
                            model: model.clone(),
                            choices: vec![wire::ChunkChoice {
                                index: 0,
                                delta: wire::DeltaMessage {
                                    role: None,
                                    content: None,
                                    reasoning_content: None,
                                    tool_calls: Some(vec![wire::ToolCall {
                                        index,
                                        id: None,
                                        call_type: "function".into(),
                                        function: wire::ToolCallFunction {
                                            name: None,
                                            arguments: Some(delta),
                                        },
                                    }]),
                                },
                                finish_reason: None,
                            }],
                            usage: None,
                        })
                        .unwrap_or_default(),
                    ));
                }
                PipelineStreamEvent::Completed { usage_cost, .. } => {
                    // finish chunk
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
                        })
                        .unwrap_or_default(),
                    ));
                    // usage chunk（choices 为空）
                    if let Some(uc) = usage_cost {
                        yield Ok(axum::response::sse::Event::default().data(
                            serde_json::to_string(&wire::ChatCompletionChunk {
                                id: id.clone(),
                                object: "chat.completion.chunk".into(),
                                created: unix_now(),
                                model: model.clone(),
                                choices: vec![],
                                usage: Some(wire::Usage {
                                    prompt_tokens: uc.usage.input_tokens,
                                    completion_tokens: uc.usage.output_tokens,
                                    total_tokens: uc.usage.total_tokens,
                                    prompt_tokens_details: Some(wire::PromptTokensDetails {
                                        cached_tokens: uc.usage.cached_input_tokens,
                                    }),
                                    completion_tokens_details: Some(wire::CompletionTokensDetails {
                                        reasoning_tokens: uc.usage.reasoning_tokens,
                                    }),
                                }),
                            })
                            .unwrap_or_default(),
                        ));
                    }
                }
                PipelineStreamEvent::Failed { code, message } => {
                    yield Ok(axum::response::sse::Event::default()
                        .event("error")
                        .data(
                            serde_json::to_string(&wire::GatewayError::new(message, "aihub_error", code.as_str()))
                                .unwrap_or_default(),
                        ));
                }
                PipelineStreamEvent::Started { .. } => {}
            }
        }
        yield Ok(axum::response::sse::Event::default().data("[DONE]"));
    };

    let mut response = axum::response::sse::Sse::new(event_stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-aih-request-id", v);
    }
    response
}

async fn embeddings_handler(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<wire::EmbeddingRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(e) => return e.into_response(),
    };
    match execute_embeddings(&state, &ctx, request).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => pipeline_error_response(&e),
    }
}
