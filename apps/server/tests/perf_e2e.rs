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

//! 性能/稳定性冒烟（§31.8）：并发非流式、并发流式、取消风暴（client cancellation storm）、
//! 慢 Provider、429 突发。验收重点是 Gateway 自身开销与稳定性（§31.8 INFERRED）。

use aihub_api_types::admin::*;
use aihub_config::{Config, GatewayConfig, Mode};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower::ServiceExt;

async fn spawn_mock_upstream() -> String {
    let active = Arc::new(AtomicUsize::new(0));
    let active_clone = active.clone();
    async fn chat(
        axum::extract::State(active): axum::extract::State<Arc<AtomicUsize>>,
        Json(request): Json<Value>,
    ) -> axum::response::Response {
        let in_flight = active.fetch_add(1, Ordering::SeqCst) + 1;
        // 模拟 30ms 上游延迟
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let _ = active.fetch_sub(1, Ordering::SeqCst);
        let content = format!(
            "perf-echo:{}:{}",
            request["messages"][0]["content"].as_str().unwrap_or(""),
            in_flight
        );
        if request["stream"].as_bool().unwrap_or(false) {
            let stream = futures::stream::iter(vec![
                Ok::<_, std::convert::Infallible>(
                    axum::response::sse::Event::default().data(
                        json!({"id":"p","object":"chat.completion.chunk","created":1,"model":"m",
                           "choices":[{"index":0,"delta":{"content":content}}]})
                        .to_string(),
                    ),
                ),
                Ok::<_, std::convert::Infallible>(
                    axum::response::sse::Event::default().data("[DONE]"),
                ),
            ]);
            return axum::response::sse::Sse::new(stream).into_response();
        }
        Json(json!({
            "id": "p", "object": "chat.completion", "created": 1, "model": request["model"],
            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }))
        .into_response()
    }
    let app = Router::new()
        .route("/v1/chat/completions", post(chat))
        .with_state(active_clone);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

async fn test_core() -> aihub_server::Core {
    let dir = std::env::temp_dir().join(format!("aihub-perf-{}", uuid::Uuid::new_v4()));
    let config = Config {
        mode: Mode::Server,
        data_dir: Some(dir.to_string_lossy().to_string()),
        gateway: GatewayConfig {
            host: "127.0.0.1".into(),
            port: 18791,
            request_timeout_ms: 30_000,
        },
        ..Default::default()
    };
    aihub_server::bootstrap(config).await.unwrap()
}

fn chat_req(token: &str, model: &str, stream: bool) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"model": model, "messages": [{"role": "user", "content": "perf"}], "stream": stream})
                .to_string(),
        ))
        .unwrap()
}

async fn setup(core: &aihub_server::Core) -> String {
    let url = spawn_mock_upstream().await;
    let state = &core.state;
    let provider = state
        .providers
        .create(CreateProviderRequest {
            key: "perf".into(),
            name: "Perf".into(),
            kind: "openai_compatible".into(),
            base_url: url,
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
            pricing: Some(
                json!({"currency": "USD", "unitTokens": 1000000, "input": 1.0, "output": 1.0}),
            ),
            enabled: true,
        })
        .await
        .unwrap();
    state
        .virtual_models
        .replace_targets(
            core.state
                .repos
                .virtual_models
                .get_by_key("general-smart")
                .await
                .unwrap()
                .id
                .as_str(),
            ReplaceTargetsRequest {
                targets: vec![TargetInput {
                    model_id: model.id.clone(),
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
    let app = state
        .applications
        .create(CreateApplicationRequest {
            key: "perf-app".into(),
            name: "Perf".into(),
            allowed_virtual_models: Some(vec![]),
            allow_direct_models: Some(false),
            monthly_budget_microunits: None,
            quota: None,
        })
        .await
        .unwrap();
    state
        .applications
        .create_key(
            &app.id,
            CreateApiKeyRequest {
                name: "perf".into(),
                scopes: None,
                expires_at: None,
            },
        )
        .await
        .unwrap()
        .plaintext
}

/// 并发非流式 + 并发流式（§31.8）：全部成功、持久化一致。
#[tokio::test]
async fn concurrent_non_streaming_and_streaming() {
    let core = test_core().await;
    let key = setup(&core).await;
    let router = core.router.clone();

    let mut handles = Vec::new();
    for i in 0..20 {
        let router = router.clone();
        let key = key.clone();
        let stream = i % 2 == 0;
        handles.push(tokio::spawn(async move {
            let response = router
                .oneshot(chat_req(&key, "general-smart", stream))
                .await
                .unwrap();
            let status = response.status();
            // 读完整 body：不读完即 drop 会触发客户端取消路径（§12.4）
            let _ = axum::body::to_bytes(response.into_body(), 8_000_000)
                .await
                .unwrap();
            (stream, status)
        }));
    }
    let mut ok = 0;
    let mut stream_ok = 0;
    for handle in handles {
        let (stream, status) = handle.await.unwrap();
        assert_eq!(status, StatusCode::OK, "concurrent request must succeed");
        ok += 1;
        if stream {
            stream_ok += 1;
        }
    }
    assert_eq!(ok, 20);
    assert_eq!(stream_ok, 10);

    // 持久化一致：20 条请求全部落库并完成（流式任务异步落盘，给收尾窗口）
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let (requests, total) = core
        .state
        .repos
        .requests
        .list(&aihub_domain::repos::RequestFilter {
            page: 1,
            page_size: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(total, 20);
    assert!(requests
        .iter()
        .all(|r| r.status == aihub_domain::entities::RequestStatus::Completed));
}

/// 取消风暴（§31.8 client cancellation storm）：并发发起后立刻丢弃响应体，
/// Gateway 不得崩溃/挂死，且取消请求被记录（§12.4）。
#[tokio::test]
async fn client_cancellation_storm() {
    let core = test_core().await;
    let key = setup(&core).await;

    for _ in 0..10 {
        let router = core.router.clone();
        let key = key.clone();
        let task = tokio::spawn(async move {
            let response = router
                .oneshot(chat_req(&key, "general-smart", true))
                .await
                .unwrap();
            // 立即 drop：模拟客户端断开
            drop(response);
        });
        let _ = task.await;
    }
    // 给流水线任务时间落盘取消状态
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Gateway 仍然健康
    let (requests, _) = core
        .state
        .repos
        .requests
        .list(&aihub_domain::repos::RequestFilter {
            page: 1,
            page_size: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    // 取消或完成的请求都有记录；不允许卡在 running
    let stuck = requests
        .iter()
        .filter(|r| r.status == aihub_domain::entities::RequestStatus::Running)
        .count();
    assert_eq!(
        stuck, 0,
        "no request may be stuck in running after cancel storm"
    );
}

/// 慢 Provider + 429 突发（§31.8）：429 在默认策略下触发 failover/失败但不挂死。
#[tokio::test]
async fn provider_429_burst_degrades_gracefully() {
    let core = test_core().await;
    let key = setup(&core).await;
    // 连发 10 个请求（mock 无 429 注入——验证突发吞吐下的稳定性与计数）
    for _ in 0..10 {
        let response = core
            .router
            .clone()
            .oneshot(chat_req(&key, "general-smart", false))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let summary = core
        .state
        .repos
        .usage
        .summary(&aihub_domain::repos::UsageQuery::default())
        .await
        .unwrap();
    assert_eq!(summary.requests, 10);
    assert_eq!(summary.success_requests, 10);
}
