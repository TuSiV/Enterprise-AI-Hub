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

//! Server E2E 补充（§31.7）：多用户权限差异 + 反向代理 trusted header（§31.7 reverse proxy headers）。

use aihub_config::{Config, GatewayConfig, Mode};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn test_core(trusted_header: bool) -> aihub_server::Core {
    test_core_with_origins(trusted_header, vec![]).await
}

async fn test_core_with_origins(trusted_header: bool, origins: Vec<String>) -> aihub_server::Core {
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
            cors_allowed_origins: origins,
            secret_backend: Some("memory".into()),
            ..Default::default()
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
            "",
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
            "",
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
    assert_eq!(status, StatusCode::FORBIDDEN);
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

#[tokio::test]
async fn management_permissions_are_enforced_for_every_role() {
    let core = test_core(false).await;
    for role in [
        "end_user",
        "developer",
        "security_auditor",
        "ai_admin",
        "super_admin",
    ] {
        let user = core
            .state
            .iam
            .create_user(role, role, None, role)
            .await
            .unwrap();
        let token = core.state.iam.issue_session(&user).await;
        for (path, allowed) in [
            ("/api/v1/admin/config", role != "end_user"),
            (
                "/api/v1/admin/providers",
                matches!(role, "security_auditor" | "ai_admin" | "super_admin"),
            ),
            (
                "/api/v1/admin/prompts",
                matches!(role, "developer" | "ai_admin" | "super_admin"),
            ),
            ("/api/v1/admin/users", role == "super_admin"),
            ("/api/v1/admin/mcp-servers", role == "super_admin"),
        ] {
            let (status, body) = send(&core.router, req("GET", path, &token, None)).await;
            assert_eq!(
                status,
                if allowed {
                    StatusCode::OK
                } else {
                    StatusCode::FORBIDDEN
                },
                "{role} {path}: {body}"
            );
        }
        if role != "super_admin" {
            for path in [
                "/api/v1/admin/users",
                "/api/v1/admin/mcp-servers",
                "/api/v1/admin/security/policies",
            ] {
                let (status, _) =
                    send(&core.router, req("POST", path, &token, Some(json!({})))).await;
                assert_eq!(status, StatusCode::FORBIDDEN, "{role} {path}");
            }
        }
        core.state.iam.revoke_session(&token).await;
        let (status, _) = send(
            &core.router,
            req("GET", "/api/v1/admin/config", &token, None),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn disabled_trusted_users_and_invalid_explicit_tokens_are_rejected() {
    let core = test_core(true).await;
    let user = core
        .state
        .iam
        .identity_from_trusted_header("employee", "Employee")
        .await
        .unwrap();
    core.state
        .repos
        .users
        .assign_role(&user.id, "super_admin")
        .await
        .unwrap();
    let trusted = || {
        Request::builder()
            .uri("/api/v1/admin/users")
            .header("x-aih-user-id", "employee")
    };
    let (status, _) = send(&core.router, trusted().body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &core.router,
        trusted()
            .header("authorization", "Bearer wrong")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    core.state
        .repos
        .users
        .set_status(&user.id, "disabled")
        .await
        .unwrap();
    let (status, _) = send(&core.router, trusted().body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn legacy_passwords_upgrade_on_login_and_logout_revokes_session() {
    let core = test_core(false).await;
    let user = core
        .state
        .iam
        .create_user("legacy", "Legacy", Some("old-password"), "developer")
        .await
        .unwrap();
    assert!(user
        .password_hash
        .as_ref()
        .unwrap()
        .starts_with("$argon2id$"));
    let legacy = format!(
        "salt${}",
        aihub_application::iam_service::hash_password("old-password", "salt")
    );
    core.state
        .repos
        .users
        .set_password_hash(&user.id, &legacy)
        .await
        .unwrap();
    assert!(core.state.iam.login("legacy", "wrong").await.is_err());
    assert_eq!(
        core.state
            .repos
            .users
            .get(&user.id)
            .await
            .unwrap()
            .password_hash,
        Some(legacy)
    );
    let (status, body) = send(
        &core.router,
        req(
            "POST",
            "/api/v1/auth/login",
            "",
            Some(json!({"username": "legacy", "password": "old-password"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["data"]["token"].as_str().unwrap();
    assert!(core
        .state
        .repos
        .users
        .get(&user.id)
        .await
        .unwrap()
        .password_hash
        .unwrap()
        .starts_with("$argon2id$"));
    assert!(core.state.iam.login("legacy", "old-password").await.is_ok());
    assert!(core.state.iam.login("legacy", "wrong").await.is_err());
    let (status, _) = send(
        &core.router,
        req("POST", "/api/v1/auth/logout", token, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(core.state.iam.session_user(token).await.is_none());
    core.state
        .repos
        .users
        .set_status(&user.id, "disabled")
        .await
        .unwrap();
    assert!(core
        .state
        .iam
        .login("legacy", "old-password")
        .await
        .is_err());
}

#[tokio::test]
async fn default_cors_does_not_allow_arbitrary_browser_origins() {
    let core = test_core(false).await;
    let response = core
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v1/admin/users")
                .header("origin", "https://untrusted.example")
                .header("access-control-request-method", "POST")
                .header("access-control-request-headers", "authorization")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());
    assert!(response.headers().contains_key("content-security-policy"));
}

#[tokio::test]
async fn session_expires_after_eight_hours() {
    let core = test_core(false).await;
    let user = core
        .state
        .iam
        .create_user("session-expiry", "Expiry", None, "developer")
        .await
        .unwrap();
    let token = core.state.iam.issue_session(&user).await;
    assert!(core.state.iam.session_user(&token).await.is_some());
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(8 * 60 * 60)).await;
    assert!(core.state.iam.session_user(&token).await.is_none());
    tokio::time::resume();
}

#[tokio::test]
async fn cors_only_allows_configured_exact_origins_and_safe_headers() {
    let core = test_core_with_origins(false, vec!["https://console.example".into()]).await;
    for (origin, allowed) in [
        ("https://console.example", true),
        ("https://console.example.evil", false),
    ] {
        let response = core
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/v1/admin/config")
                    .header("origin", origin)
                    .header("access-control-request-method", "GET")
                    .header("access-control-request-headers", "authorization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_some(),
            allowed
        );
        let headers = response
            .headers()
            .get("access-control-allow-headers")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!headers.contains("x-aih-user-id"));
    }
}

#[tokio::test]
async fn public_login_attempts_are_rate_limited() {
    let core = test_core(false).await;
    for _ in 0..60 {
        let (status, _) = send(
            &core.router,
            req(
                "POST",
                "/api/v1/auth/login",
                "",
                Some(json!({"username": "missing", "password": "wrong"})),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = send(
        &core.router,
        req(
            "POST",
            "/api/v1/auth/login",
            "",
            Some(json!({"username": "missing", "password": "wrong"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
