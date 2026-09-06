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

//! 平台能力 e2e（M8 Prompt / M12-13 Knowledge / M14 Agent / M16 Eval / M17 Security / M10 IAM）。

use aihub_api_types::admin::*;
use aihub_config::{Config, GatewayConfig, Mode};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower::ServiceExt;

/// 确定性向量 mock：embed(text) = [len/8, contains(北京), contains(上海), contains(合同)]
/// 使"北京"类查询对"北京"文档相似度更高，可验证检索排序。
fn feature_vector(text: &str) -> Vec<f32> {
    vec![
        (text.chars().count() / 8).min(50) as f32,
        if text.contains("北京") { 10.0 } else { 0.0 },
        if text.contains("上海") { 10.0 } else { 0.0 },
        if text.contains("合同") { 10.0 } else { 0.0 },
    ]
}

async fn spawn_chat_mock() -> String {
    async fn models() -> Json<Value> {
        Json(json!({"object": "list", "data": [
            {"id": "mock-mini", "object": "model"},
            {"id": "mock-embed", "object": "model"}
        ]}))
    }
    async fn chat(Json(request): Json<Value>) -> axum::response::Response {
        let content = format!(
            "mock-echo: {}",
            request["messages"]
                .as_array()
                .and_then(|m| m.last())
                .and_then(|m| m["content"].as_str())
                .unwrap_or("")
        );
        Json(json!({
            "id": "mock-1", "object": "chat.completion", "created": 1, "model": request["model"],
            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20}
        }))
        .into_response()
    }
    async fn embeddings(Json(request): Json<Value>) -> Json<Value> {
        let inputs: Vec<Value> = match &request["input"] {
            Value::Array(items) => items.clone(),
            other => vec![other.clone()],
        };
        let data: Vec<Value> = inputs
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let text = v.as_str().unwrap_or_default();
                json!({"object": "embedding", "embedding": feature_vector(text), "index": i})
            })
            .collect();
        Json(
            json!({"object": "list", "data": data, "model": request["model"],
                    "usage": {"prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5}}),
        )
    }
    let app = Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/embeddings", post(embeddings));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

async fn test_core() -> aihub_server::Core {
    let dir = std::env::temp_dir().join(format!("aihub-platform-{}", uuid::Uuid::new_v4()));
    let config = Config {
        mode: Mode::Server,
        data_dir: Some(dir.to_string_lossy().to_string()),
        gateway: GatewayConfig {
            host: "127.0.0.1".into(),
            port: 18789,
            request_timeout_ms: 30_000,
        },
        ..Default::default()
    };
    aihub_server::bootstrap(config).await.unwrap()
}

fn http_req(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    match body {
        Some(v) => builder
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8_000_000)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

/// 公共准备：mock provider + 模型（chat/embed）+ general-smart 绑定 + 返回 admin token
async fn setup_world(core: &aihub_server::Core) -> String {
    let state = &core.state;
    let url = spawn_chat_mock().await;
    let provider = state
        .providers
        .create(CreateProviderRequest {
            key: "plat".into(),
            name: "Platform Mock".into(),
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
    let chat_model = state
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
    let embed_model = state
        .models
        .create(CreateModelRequest {
            provider_id: provider.id.clone(),
            model_key: "mock-embed".into(),
            display_name: "Embed".into(),
            model_type: "embedding".into(),
            context_window: None,
            max_output_tokens: None,
            capabilities: Some(json!({})),
            pricing: Some(
                json!({"currency": "USD", "unitTokens": 1000000, "input": 0.1, "output": 0.0}),
            ),
            enabled: true,
        })
        .await
        .unwrap();
    // 绑定 general-smart → chat model
    let vm = state
        .repos
        .virtual_models
        .get_by_key("general-smart")
        .await
        .unwrap();
    state
        .virtual_models
        .replace_targets(
            &vm.id,
            ReplaceTargetsRequest {
                targets: vec![TargetInput {
                    model_id: chat_model.id.clone(),
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
    let _ = embed_model.id;
    core.admin_token.clone()
}

#[tokio::test]
async fn prompt_center_lifecycle() {
    let core = test_core().await;
    let token = setup_world(&core).await;
    let router = &core.router;

    // 创建 → 新版本 → publish → 旧 published 自动 deprecated
    let (status, body) = send(
        router,
        http_req(
            "POST",
            "/api/v1/admin/prompts",
            &token,
            Some(json!({
                "key": "legal-summary", "name": "法律摘要", "description": "合同摘要模板"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let prompt_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/prompts/{prompt_id}/versions"),
            &token,
            Some(json!({
                "systemTemplate": "你是法律助手", "userTemplate": "请总结：{{document}}"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v1 = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["version"], 1);

    let (status, body) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/prompt-versions/{v1}/publish"),
            &token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "published");

    // v2 发布后 v1 废弃（唯一 published，可追溯）
    let (status, body) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/prompts/{prompt_id}/versions"),
            &token,
            Some(json!({
                "systemTemplate": "你是法律助手 v2", "userTemplate": "请总结：{{document}}"
            })),
        ),
    )
    .await;
    let v2 = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/prompt-versions/{v2}/publish"),
            &token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, detail) = send(
        router,
        http_req(
            "GET",
            &format!("/api/v1/admin/prompts/{prompt_id}"),
            &token,
            None,
        ),
    )
    .await;
    assert_eq!(detail["data"]["publishedVersion"], 2);
    let v1_status = detail["data"]["versions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"].as_str() == Some(v1.as_str()))
        .unwrap()["status"]
        .clone();
    assert_eq!(v1_status, "deprecated");
}

#[tokio::test]
async fn knowledge_upload_and_retrieval() {
    let core = test_core().await;
    let token = setup_world(&core).await;
    let router = &core.router;

    // 建 KB + 绑定 embedding 模型
    let (_, models_body) = send(
        router,
        http_req("GET", "/api/v1/admin/models", &token, None),
    )
    .await;
    let embed_model_id = models_body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["modelKey"] == "mock-embed")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, kb_body) = send(router, http_req("POST", "/api/v1/admin/knowledge-bases", &token, Some(json!({
        "key": "legal-kb", "name": "法务知识库", "visibility": "internal", "embeddingModelId": embed_model_id
    })))).await;
    assert_eq!(status, StatusCode::OK, "{kb_body}");
    let kb_id = kb_body["data"]["id"].as_str().unwrap().to_string();

    // 上传文档（北京相关）
    let (status, doc_body) = send(router, http_req("POST", &format!("/api/v1/admin/documents/{kb_id}/upload-content"), &token, Some(json!({
        "filename": "beijing-office.txt", "mimeType": "text/plain",
        "content": "北京办公室管理制度。\n\n北京办公室位于朝阳区，工位需要提前预订。\n\n上海办公室在浦东新区。"
    })))).await;
    assert_eq!(status, StatusCode::OK, "{doc_body}");
    assert_eq!(doc_body["data"]["parse_status"], "ready");

    // 重复上传同内容 → 冲突拒绝（§19.1 去重）
    let (status, _) = send(router, http_req("POST", &format!("/api/v1/admin/documents/{kb_id}/upload-content"), &token, Some(json!({
        "filename": "dup.txt", "mimeType": "text/plain",
        "content": "北京办公室管理制度。\n\n北京办公室位于朝阳区，工位需要提前预订。\n\n上海办公室在浦东新区。"
    })))).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 检索："北京" 查询应命中 北京文档内容 且分数高于无关内容
    let (status, query_body) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/knowledge-bases/{kb_id}/query"),
            &token,
            Some(json!({
                "query": "北京办公室 工位 预订", "topK": 3
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{query_body}");
    let hits = query_body["data"]["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0]["content"].as_str().unwrap().contains("北京"));
    // 引用定位字段（§19.5）
    assert!(hits[0]["filename"].is_string());
    let context = query_body["data"]["context"].as_str().unwrap();
    assert!(context.contains("[1]"));
}

#[tokio::test]
async fn agent_run_with_tools() {
    let core = test_core().await;
    let token = setup_world(&core).await;
    let router = &core.router;

    // 创建 Agent（允许 echo/now 工具）→ publish → run
    let (status, body) = send(
        router,
        http_req(
            "POST",
            "/api/v1/admin/agents",
            &token,
            Some(json!({
                "key": "assistant", "name": "助手", "description": "测试 Agent",
                "modelRef": "general-smart", "systemPrompt": "你是测试助手", "maxSteps": 4,
                "allowedTools": ["echo", "now"]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let agent_id = body["data"]["agent"]["id"].as_str().unwrap().to_string();
    let version_id = body["data"]["version"]["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/agent-versions/{version_id}/publish"),
            &token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, run_body) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/agents/{agent_id}/run"),
            &token,
            Some(json!({
                "input": {"task": "请介绍一下你自己"}
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run_body}");
    // mock 上游不产生 tool_calls，agent 应单步完成
    assert_eq!(run_body["data"]["status"], "completed");
    assert!(run_body["data"]["output"]
        .as_str()
        .unwrap()
        .contains("mock-echo"));

    // run 记录可查
    let run_id = run_body["data"]["runId"].as_str().unwrap().to_string();
    let (status, detail) = send(
        router,
        http_req(
            "GET",
            &format!("/api/v1/admin/agent-runs/{run_id}"),
            &token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["data"]["run"]["status"], "completed");
}

#[tokio::test]
async fn eval_dataset_and_run() {
    let core = test_core().await;
    let token = setup_world(&core).await;
    let router = &core.router;

    let (status, ds_body) = send(
        router,
        http_req(
            "POST",
            "/api/v1/admin/evals/datasets",
            &token,
            Some(json!({
                "key": "smoke-eval", "name": "冒烟评测集"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ds_body}");
    let dataset_id = ds_body["data"]["id"].as_str().unwrap().to_string();

    // mock 回显最后一条消息 → contains "hello" 命中
    let (status, _) = send(router, http_req("POST", &format!("/api/v1/admin/evals/datasets/{dataset_id}/cases"), &token, Some(json!({
        "caseName": "greet", "input": {"question": "请输出 hello"}, "expectedOutput": "hello"
    })))).await;
    assert_eq!(status, StatusCode::OK);

    let (status, run_body) = send(
        router,
        http_req(
            "POST",
            &format!("/api/v1/admin/evals/datasets/{dataset_id}/runs"),
            &token,
            Some(json!({
                "label": "baseline", "candidate": {"model": "general-smart"},
                "judgeConfig": {"rule": {"contains": ["hello"]}}
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run_body}");
    assert_eq!(run_body["data"]["status"], "completed");
    let avg = run_body["data"]["summary"]["avgScore"].as_f64().unwrap();
    assert!(avg > 0.5, "avgScore={avg}");

    // 回归查看：run 详情带 results
    let run_id = run_body["data"]["runId"].as_str().unwrap().to_string();
    let (status, detail) = send(
        router,
        http_req(
            "GET",
            &format!("/api/v1/admin/evals/runs/{run_id}"),
            &token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["data"]["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn security_dlp_and_iam() {
    let core = test_core().await;
    let token = setup_world(&core).await;
    let router = &core.router;

    // DLP：手机号脱敏
    let (status, dlp) = send(
        router,
        http_req(
            "POST",
            "/api/v1/admin/security/dlp/scan",
            &token,
            Some(json!({
                "content": "联系人 13812345678 请回电"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{dlp}");
    assert_eq!(dlp["data"]["action"], "mask");
    let masked = dlp["data"]["content"].as_str().unwrap();
    assert!(masked.contains("138****"));
    assert!(!masked.contains("13812345678"));

    // IAM：创建用户 → 登录 → session 可用
    let (status, user_body) = send(router, http_req("POST", "/api/v1/admin/users", &token, Some(json!({
        "username": "alice", "displayName": "Alice", "password": "secret123", "role": "developer"
    })))).await;
    assert_eq!(status, StatusCode::OK, "{user_body}");

    let (status, login) = send(
        router,
        http_req(
            "POST",
            "/api/v1/auth/login",
            &token,
            Some(json!({
                "username": "alice", "password": "secret123"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login}");
    let session = login["data"]["token"].as_str().unwrap();
    assert!(session.starts_with("aih_session_"));

    // 错误口令拒绝
    let (status, _) = send(
        router,
        http_req(
            "POST",
            "/api/v1/auth/login",
            &token,
            Some(json!({
                "username": "alice", "password": "wrong"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // session token 能过 admin 鉴权（替代 admin token 访问 config）
    let (status, _) = send(
        router,
        http_req("GET", "/api/v1/admin/config", session, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
