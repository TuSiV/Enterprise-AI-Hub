//! Server E2E 补充（§31.7）：多用户权限差异 + 反向代理 trusted header（§31.7 reverse proxy headers）。

use aihub_config::{Config, GatewayConfig, Mode};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn test_core(trusted_header: bool) -> aihub_server::Core {
    let dir = std::env::temp_dir().join(format!("aihub-rbac-{}", uuid::Uuid::new_v4()));
    let config = Config {
        mode: Mode::Server,
        data_dir: Some(dir.to_string_lossy().to_string()),
        gateway: GatewayConfig {
            host: "127.0.0.1".into(),
            port: 18790,
            request_timeout_ms: 30_000,
        },
        auth: aihub_config::AuthConfig {
            admin_token: None,
            trusted_header_user: trusted_header,
            secret_backend: Some("memory".into()),
        },
        ..Default::default()
    };
    aihub_server::bootstrap(config).await.unwrap()
}

fn req(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json");
    match body {
        Some(v) => builder.body(Body::from(v.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn send(router: &Router, r: Request<Body>) -> (StatusCode, Value) {
    let response: axum::response::Response = router.clone().oneshot(r).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// §31.7 multi-user permission：用户创建 → 登录 → session 访问；
/// trusted_header 关闭时伪造 X-AIH-User-ID 不得获得访问权（§11.3）。
#[tokio::test]
async fn multi_user_rbac_login_and_trusted_header_gate() {
    let core = test_core(false).await;
    let admin = core.admin_token.clone();

    // 创建两个用户（不同角色）
    let (status, _) = send(
        &core.router,
        req(
            "POST",
            "/api/v1/admin/users",
            &admin,
            Some(json!({
                "username": "dev1", "displayName": "Dev1", "password": "pw1", "role": "developer"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &core.router,
        req("POST", "/api/v1/admin/users", &admin, Some(json!({
            "username": "auditor1", "displayName": "Aud", "password": "pw2", "role": "security_auditor"
        })),
    )).await;
    assert_eq!(status, StatusCode::OK);

    // 双用户登录
    let (status, login1) = send(
        &core.router,
        req(
            "POST",
            "/api/v1/auth/login",
            &admin,
            Some(json!({
                "username": "dev1", "password": "pw1"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session1 = login1["data"]["token"].as_str().unwrap();
    let (status, login2) = send(
        &core.router,
        req(
            "POST",
            "/api/v1/auth/login",
            &admin,
            Some(json!({
                "username": "auditor1", "password": "pw2"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session2 = login2["data"]["token"].as_str().unwrap();

    // 两个 session 都能过鉴权读 config
    let (status, _) = send(
        &core.router,
        req("GET", "/api/v1/admin/config", session1, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &core.router,
        req("GET", "/api/v1/admin/config", session2, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // trusted_header_user=false：伪造 X-AIH-User-ID 不能过鉴权（§11.3 防伪造）
    let forged = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/config")
        .header("x-aih-user-id", "attacker")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&core.router, forged).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// §31.7 reverse proxy headers：trusted_header_user=true 时反代注入的 X-AIH-User-ID
/// 自动映射身份并放行（§14.2 TrustedHeaderIdentityProvider）。
#[tokio::test]
async fn trusted_header_identity_provider_flow() {
    let core = test_core(true).await;
    let forged = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/config")
        .header("x-aih-user-id", "employee-42")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&core.router, forged).await;
    assert_eq!(status, StatusCode::OK);
    // 身份已落库（users 表自动建号 + end_user 角色）
    let user = core
        .state
        .repos
        .users
        .get_by_subject("trusted_header", "employee-42")
        .await
        .unwrap()
        .expect("identity auto-provisioned");
    assert_eq!(user.identity_provider, "trusted_header");
}
