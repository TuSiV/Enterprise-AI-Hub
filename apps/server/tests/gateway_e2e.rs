//! Gateway Integration Test（方案 §31.4）：API Key 鉴权、Virtual Model 解析、
//! Retry/Failover、Streaming、Usage/Cost 持久化、Quota、Admin API。

use aihub_api_types::admin::*;
use aihub_config::{Config, Mode, GatewayConfig};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower::ServiceExt;

// ---------- mock 上游 ----------

#[derive(Clone, Copy)]
enum MockBehavior {
    Ok,
    Fail500,
    Fail429,
}

async fn spawn_mock(behavior: MockBehavior) -> String {
    async fn models() -> Json<Value> {
        Json(json!({"object": "list", "data": [
            {"id": "mock-mini", "object": "model", "owned_by": "mock"},
            {"id": "mock-embed", "object": "model", "owned_by": "mock"}
        ]}))
    }

    async fn chat(
        axum::extract::State(behavior): axum::extract::State<MockBehavior>,
        Json(request): Json<Value>,
    ) -> axum::response::Response {
        match behavior {
            MockBehavior::Fail500 => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": {"message": "mock 500"}})))
                    .into_response();
            }
            MockBehavior::Fail429 => {
                return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": {"message": "mock 429"}})))
                    .into_response();
            }
            MockBehavior::Ok => {}
        }
        let content = format!(
            "mock-echo: {}",
            request["messages"][0]["content"].as_str().unwrap_or("")
        );

        // 流式请求返回 SSE
        if request["stream"].as_bool().unwrap_or(false) {
            let id = "mock-stream-1";
            let model = request["model"].clone();
            let words: Vec<String> = content.split_inclusive(' ').map(|s| s.to_string()).collect();
            let stream = async_stream::stream! {
                for word in words {
                    yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(
                        json!({"id": id, "object": "chat.completion.chunk", "created": 1, "model": model,
                               "choices": [{"index": 0, "delta": {"content": word}}]})
                        .to_string(),
                    ));
                }
                yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(
                    json!({"id": id, "object": "chat.completion.chunk", "created": 1, "model": model,
                           "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})
                    .to_string(),
                ));
                yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(
                    json!({"id": id, "object": "chat.completion.chunk", "created": 1, "model": model,
                           "choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20}})
                    .to_string(),
                ));
                yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data("[DONE]"));
            };
            return axum::response::sse::Sse::new(stream)
                .keep_alive(axum::response::sse::KeepAlive::default())
                .into_response();
        }

        Json(json!({
            "id": "mock-1", "object": "chat.completion", "created": 1, "model": request["model"],
            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20}
        }))
        .into_response()
    }

    async fn embeddings(Json(request): Json<Value>) -> Json<Value> {
        let count = match &request["input"] {
            Value::Array(items) => items.len(),
            _ => 1,
        };
        let data: Vec<Value> = (0..count)
            .map(|i| json!({"object": "embedding", "embedding": [0.1, 0.2], "index": i}))
            .collect();
        Json(json!({
            "object": "list",
            "data": data,
            "model": request["model"],
            "usage": {"prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5}
        }))
    }

    let app = Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/embeddings", post(embeddings))
        .with_state(behavior);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

async fn test_core() -> (aihub_server::Core, Config) {
    let dir = std::env::temp_dir().join(format!("aihub-e2e-{}", uuid::Uuid::new_v4()));
    let config = Config {
        mode: Mode::Server,
        data_dir: Some(dir.to_string_lossy().to_string()),
        gateway: GatewayConfig {
            host: "127.0.0.1".into(),
            port: 18787,
            request_timeout_ms: 30_000,
        },
        ..Default::default()
    };
    let core = aihub_server::bootstrap(config.clone()).await.unwrap();
    (core, config)
}

fn http_req(method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    match body {
        Some(value) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8_000_000).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };
    (status, body)
}

async fn setup_world(core: &aihub_server::Core) -> String {
    let state = &core.state;
    let ok_url = spawn_mock(MockBehavior::Ok).await;
    let fail_url = spawn_mock(MockBehavior::Fail500).await;

    // Provider A：始终 500；Provider B：正常
    let provider_a = state
        .providers
        .create(CreateProviderRequest {
            key: "mock-a".into(),
            name: "Mock A".into(),
            kind: "openai_compatible".into(),
            base_url: fail_url,
            api_key: None,
            timeout_ms: Some(10_000),
            max_retries: Some(2),
            enabled: true,
            config: json!({}),
        })
        .await
        .unwrap();
    let provider_b = state
        .providers
        .create(CreateProviderRequest {
            key: "mock-b".into(),
            name: "Mock B".into(),
            kind: "openai_compatible".into(),
            base_url: ok_url,
            api_key: None,
            timeout_ms: Some(10_000),
            max_retries: Some(2),
            enabled: true,
            config: json!({}),
        })
        .await
        .unwrap();

    let chat_pricing = json!({"currency": "USD", "unitTokens": 1000000, "input": 1.0, "output": 2.0});
    let model_a = state
        .models
        .create(CreateModelRequest {
            provider_id: provider_a.id.clone(),
            model_key: "mock-mini".into(),
            display_name: "Mock Mini A".into(),
            model_type: "chat".into(),
            context_window: None,
            max_output_tokens: None,
            capabilities: Some(json!({})),
            pricing: Some(chat_pricing.clone()),
            enabled: true,
        })
        .await
        .unwrap();
    let model_b = state
        .models
        .create(CreateModelRequest {
            provider_id: provider_b.id.clone(),
            model_key: "mock-mini".into(),
            display_name: "Mock Mini B".into(),
            model_type: "chat".into(),
            context_window: None,
            max_output_tokens: None,
            capabilities: Some(json!({})),
            pricing: Some(chat_pricing.clone()),
            enabled: true,
        })
        .await
        .unwrap();
    let embed_model = state
        .models
        .create(CreateModelRequest {
            provider_id: provider_b.id.clone(),
            model_key: "mock-embed".into(),
            display_name: "Mock Embed".into(),
            model_type: "embedding".into(),
            context_window: None,
            max_output_tokens: None,
            capabilities: Some(json!({})),
            pricing: Some(json!({"currency": "USD", "unitTokens": 1000000, "input": 0.1, "output": 0.0})),
            enabled: true,
        })
        .await
        .unwrap();
    let _ = (&model_a, &embed_model);

    // Virtual Model：A 优先，B 兜底（failover 场景）
    // general-smart / embedding-default 已由 seed 预置：存在则替换 targets
    let existing = state.repos.virtual_models.get_by_key("general-smart").await.ok();
    let general_smart = match existing {
        Some(vm) => {
            state
                .virtual_models
                .replace_targets(
                    &vm.id,
                    ReplaceTargetsRequest {
                        targets: vec![
                            TargetInput {
                                model_id: model_a.id.clone(),
                                priority: Some(10),
                                weight: None,
                                enabled: true,
                                condition: None,
                                overrides: None,
                            },
                            TargetInput {
                                model_id: model_b.id.clone(),
                                priority: Some(20),
                                weight: None,
                                enabled: true,
                                condition: None,
                                overrides: None,
                            },
                        ],
                    },
                )
                .await
                .unwrap()
        }
        None => state
            .virtual_models
            .create(CreateVirtualModelRequest {
                key: "general-smart".into(),
                name: "General Smart".into(),
                description: None,
                routing_strategy: "priority_failover".into(),
                enabled: true,
                config: Some(json!({"retry": {"maxAttemptsPerTarget": 2, "retry5xx": true}})),
                targets: vec![
                    TargetInput {
                        model_id: model_a.id.clone(),
                        priority: Some(10),
                        weight: None,
                        enabled: true,
                        condition: None,
                        overrides: None,
                    },
                    TargetInput {
                        model_id: model_b.id.clone(),
                        priority: Some(20),
                        weight: None,
                        enabled: true,
                        condition: None,
                        overrides: None,
                    },
                ],
            })
            .await
            .unwrap(),
    };
    let _ = &general_smart;

    // embedding virtual model
    let existing = state.repos.virtual_models.get_by_key("embedding-default").await.ok();
    match existing {
        Some(vm) => {
            state
                .virtual_models
                .replace_targets(
                    &vm.id,
                    ReplaceTargetsRequest {
                        targets: vec![TargetInput {
                            model_id: embed_model.id.clone(),
                            priority: Some(10),
                            weight: None,
                            enabled: true,
                            condition: None,
                            overrides: None,
                        }],
                    },
                )
                .await
                .unwrap();
        }
        None => {
            state
                .virtual_models
                .create(CreateVirtualModelRequest {
                    key: "embedding-default".into(),
                    name: "Embedding Default".into(),
                    description: None,
                    routing_strategy: "priority_failover".into(),
                    enabled: true,
                    config: Some(json!({})),
                    targets: vec![TargetInput {
                        model_id: embed_model.id.clone(),
                        priority: Some(10),
                        weight: None,
                        enabled: true,
                        condition: None,
                        overrides: None,
                    }],
                })
                .await
                .unwrap();
        }
    }

    // 主测试应用
    let app = state
        .applications
        .create(CreateApplicationRequest {
            key: "test-app".into(),
            name: "Test App".into(),
            allowed_virtual_models: Some(vec![]),
            allow_direct_models: Some(false),
            monthly_budget_microunits: None,
            quota: None,
        })
        .await
        .unwrap();
    let key = state
        .applications
        .create_key(&app.id, CreateApiKeyRequest { name: "default".into(), scopes: None, expires_at: None })
        .await
        .unwrap();
    key.plaintext
}

#[tokio::test]
async fn gateway_e2e_auth_models_chat_failover_stream() {
    let (core, _config) = test_core().await;
    let api_key = setup_world(&core).await;
    let router = &core.router;

    // 1. 无凭证 → 401
    let (status, body) = send(router, http_req("GET", "/v1/models", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "AIH_UNAUTHORIZED");

    // 2. /v1/models 包含 virtual model
    let (status, body) = send(router, http_req("GET", "/v1/models", Some(&api_key), None)).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"general-smart"));
    assert!(ids.contains(&"embedding-default"));
    // allow_direct_models=false → 不暴露物理模型
    assert!(!ids.contains(&"mock-mini"));

    // 3. 非流式 chat：A 500 两次 → failover 到 B 成功
    let (status, body) = send(
        router,
        http_req(
            "POST",
            "/v1/chat/completions",
            Some(&api_key),
            Some(json!({
                "model": "general-smart",
                "messages": [{"role": "user", "content": "hello hub"}],
                "stream": false
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["choices"][0]["message"]["content"], "mock-echo: hello hub");
    assert_eq!(body["usage"]["prompt_tokens"], 12);
    assert_eq!(body["usage"]["completion_tokens"], 8);

    // 4. usage/cost 已持久化
    let summary = core
        .state
        .repos
        .usage
        .summary(&aihub_domain::repos::UsageQuery::default())
        .await
        .unwrap();
    assert_eq!(summary.requests, 1);
    assert_eq!(summary.total_tokens, 20);
    // input 12 tokens @1.0/1M + output 8 @2.0/1M = 12 + 16 = 28 microunits... 单位换算:
    // 12/1M*1.0*1e6 = 12; 8/1M*2.0*1e6 = 16 → total 28
    assert_eq!(summary.cost_microunits, 28);

    // 5. 流式 chat：SSE 包含内容与 [DONE]
    let response = router
        .clone()
        .oneshot(http_req(
            "POST",
            "/v1/chat/completions",
            Some(&api_key),
            Some(json!({
                "model": "general-smart",
                "messages": [{"role": "user", "content": "stream it"}],
                "stream": true
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 8_000_000).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(text.contains("mock-echo:") && text.contains("stream "), "sse: {text}");
    assert!(text.contains("chat.completion.chunk"));
    assert!(text.contains("[DONE]"));

    // 6. 未知模型 → 404
    let (status, body) = send(
        router,
        http_req(
            "POST",
            "/v1/chat/completions",
            Some(&api_key),
            Some(json!({"model": "no-such-model", "messages": [{"role": "user", "content": "x"}]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "AIH_MODEL_NOT_FOUND");

    // 7. embeddings（virtual model → embedding target）
    let (status, body) = send(
        router,
        http_req(
            "POST",
            "/v1/embeddings",
            Some(&api_key),
            Some(json!({"model": "embedding-default", "input": ["hello", "world"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn gateway_e2e_revoked_key_rejected() {
    let (core, _config) = test_core().await;
    let state = &core.state;
    let ok_url = spawn_mock(MockBehavior::Ok).await;
    let provider = state
        .providers
        .create(CreateProviderRequest {
            key: "solo".into(),
            name: "Solo".into(),
            kind: "openai_compatible".into(),
            base_url: ok_url,
            api_key: None,
            timeout_ms: Some(10_000),
            max_retries: Some(1),
            enabled: true,
            config: json!({}),
        })
        .await
        .unwrap();
    let model = state
        .models
        .create(CreateModelRequest {
            provider_id: provider.id.clone(),
            model_key: "mock-mini".into(),
            display_name: "Mini".into(),
            model_type: "chat".into(),
            context_window: None,
            max_output_tokens: None,
            capabilities: Some(json!({})),
            pricing: Some(json!({"currency": "USD", "unitTokens": 1000000, "input": 1.0, "output": 1.0})),
            enabled: true,
        })
        .await
        .unwrap();
    state
        .virtual_models
        .create(
            CreateVirtualModelRequest {
                key: "solo-smart".into(),
                name: "Solo".into(),
                description: None,
                routing_strategy: "priority_failover".into(),
                enabled: true,
                config: Some(json!({})),
                targets: vec![TargetInput {
                    model_id: model.id.clone(),
                    priority: Some(1),
                    weight: None,
                    enabled: true,
                    condition: None,
                    overrides: None,
                }],
            },
        )
        .await
        .unwrap();
    let app = state
        .applications
        .create(CreateApplicationRequest {
            key: "revoked-app".into(),
            name: "Revoked App".into(),
            allowed_virtual_models: Some(vec![]),
            allow_direct_models: Some(false),
            monthly_budget_microunits: None,
            quota: None,
        })
        .await
        .unwrap();
    let key = state
        .applications
        .create_key(&app.id, CreateApiKeyRequest { name: "default".into(), scopes: None, expires_at: None })
        .await
        .unwrap();

    // 正常可用
    let (status, _) = send(
        &core.router,
        http_req(
            "POST",
            "/v1/chat/completions",
            Some(&key.plaintext),
            Some(json!({"model": "solo-smart", "messages": [{"role": "user", "content": "hi"}]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 撤销后立即拒绝（§43.1）
    state
        .applications
        .revoke_key(&app.id, &key.key.id)
        .await
        .unwrap();
    let (status, body) = send(
        &core.router,
        http_req(
            "POST",
            "/v1/chat/completions",
            Some(&key.plaintext),
            Some(json!({"model": "solo-smart", "messages": [{"role": "user", "content": "hi"}]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body: {body}");
}

#[tokio::test]
async fn gateway_e2e_rate_limit_and_admin_auth() {
    let (core, _config) = test_core().await;
    let state = &core.state;
    let ok_url = spawn_mock(MockBehavior::Ok).await;
    let provider = state
        .providers
        .create(CreateProviderRequest {
            key: "rl".into(),
            name: "RL".into(),
            kind: "openai_compatible".into(),
            base_url: ok_url,
            api_key: None,
            timeout_ms: Some(10_000),
            max_retries: Some(1),
            enabled: true,
            config: json!({}),
        })
        .await
        .unwrap();
    let model = state
        .models
        .create(CreateModelRequest {
            provider_id: provider.id.clone(),
            model_key: "mock-mini".into(),
            display_name: "Mini".into(),
            model_type: "chat".into(),
            context_window: None,
            max_output_tokens: None,
            capabilities: Some(json!({})),
            pricing: Some(json!({"currency": "USD", "unitTokens": 1000000, "input": 1.0, "output": 1.0})),
            enabled: true,
        })
        .await
        .unwrap();
    state
        .virtual_models
        .create(
            CreateVirtualModelRequest {
                key: "rl-smart".into(),
                name: "RL".into(),
                description: None,
                routing_strategy: "priority_failover".into(),
                enabled: true,
                config: Some(json!({})),
                targets: vec![TargetInput {
                    model_id: model.id.clone(),
                    priority: Some(1),
                    weight: None,
                    enabled: true,
                    condition: None,
                    overrides: None,
                }],
            },
        )
        .await
        .unwrap();
    let app = state
        .applications
        .create(CreateApplicationRequest {
            key: "limited-app".into(),
            name: "Limited".into(),
            allowed_virtual_models: Some(vec![]),
            allow_direct_models: Some(false),
            monthly_budget_microunits: None,
            quota: Some(QuotaInput {
                rpm: Some(2),
                tpm: None,
                daily_requests: None,
                monthly_tokens: None,
                monthly_cost_microunits: None,
                exceed_action: Some("block".into()),
            }),
        })
        .await
        .unwrap();
    let key = state
        .applications
        .create_key(&app.id, CreateApiKeyRequest { name: "default".into(), scopes: None, expires_at: None })
        .await
        .unwrap();

    let payload = json!({"model": "rl-smart", "messages": [{"role": "user", "content": "hi"}]});
    let (status, _) = send(
        &core.router,
        http_req("POST", "/v1/chat/completions", Some(&key.plaintext), Some(payload.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &core.router,
        http_req("POST", "/v1/chat/completions", Some(&key.plaintext), Some(payload.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 第三次触发 RPM 限制 → 429（§15.3）
    let (status, body) = send(
        &core.router,
        http_req("POST", "/v1/chat/completions", Some(&key.plaintext), Some(payload)),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "body: {body}");
    assert_eq!(body["error"]["code"], "AIH_RATE_LIMITED");

    // Admin API 鉴权
    let (status, _) = send(
        &core.router,
        http_req("GET", "/api/v1/admin/providers", None, None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = send(
        &core.router,
        http_req("GET", "/api/v1/admin/providers", Some(&core.admin_token), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}
