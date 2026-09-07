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

//! OIDC JWKS 验签 e2e（§14.2 / §31.7 OIDC mock auth）：
//! IdP JWKS 端点 + RS256 id_token → 验签 → subject 映射 + 自动建号 + end_user 角色。

use aihub_config::{Config, GatewayConfig, Mode};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::{Json, Router};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use tower::ServiceExt;

const TEST_PRIVATE_KEY_PEM: &str = r##"-----BEGIN PRIVATE KEY-----
MIIEuwIBADANBgkqhkiG9w0BAQEFAASCBKUwggShAgEAAoIBAQCU2xbUYZ+vb9zu
6WXmZ4HeP/BZXg3H+mO3mCWRZdrkwLQLaFrrLf3AaGPTe/fXHw9jHyEdQ/ccnlCf
jHodYBI+ZTLmOF3H7mOcy8Y3We3ZlMvQE9O6gI6zRf1cougkH02ProPgwStw+F8s
raP6EcraK7I2LeYHE8F6hDeJ7VTI/Rnk3EVqm9SpPpyJDx0aAM8C/MT161Miy5sG
fSf863syVqEIYDAf0ncaCy9fesRliFMFnuOZzRWYDvnu8pAUUCc3I2l/F+gBaHK7
H6mBjVv0qK64MnRN6HhYgG6zkUT+87llaRnKiUMQBFJ3Tvmehik4SPJMMS2hRXEh
1utNo2QtAgMBAAECgf9A5ONHeNVm7RoI2vdGHh4/f09YGQqf3uE/essCn1w5tudD
GdgjbHFnVI2n2m+bqwXejBVzj1McoIDZxiCrCGkk7xehwKl+CxQV6J11f0iiMnMk
OSEvMXnzLsIlPsz7zeSdaGhaR/xWarmcB36UPjo1MMYNXKJ9Oh7VO5Ws8LnaCYN0
27vTO6fxtrCnOytqODHJMRhK/LiCO6R7sX+PZLBED55eAuvGUa2FqmwqJyTh5lKL
kA28HfwiLhbBdOFX0qi30Yp5q16cWtFSpaHSfk56imugwmys69gC93REm54XV2el
BeVB5OQ3WDc1BhoCBQFfIO15RVkVufMYfVOZ6r8CgYEAzx97NJiNefYIfTQZq4rE
Y9GaN7KvlVu6651MEJ3gk6szdpMdspKO1/Q8hZkd448PNpRGRNgz3PCCnXdfYSAZ
YoTTeYgzZf75kH+eFp+CltRQDS7cvcZCKUjotcxMDjrqAh5137e7nGPXXpgnRq4b
zaB+d9GtZf7wqcI+kS3wUacCgYEAt/ugis+ukGPVSjx4eZoRo9k6YKd+oP5URp5p
NBih6qc/eB67BdFe3UQp30jSa+bOiYu0Cc25EMHlyxgWAnFD8wFxSYZe2uG4Ui2a
hiyAe9iZTWXNjb1qfZ4vhqDgFFgNm3wFtojTKki6+Zn9zzfblczPinFeXqZmLlb8
qND5TgsCgYAqOq7yEFB4F1Ovw1mgghB8kZWx2Xq3Jxa7Rzzk/mt1cChz+pbJe9wn
56IXUxqm9NoTNLQHAVXBrD/VPytxMAw86/v6AW0JVK0pavuefTMw8WTI608SWkPP
CxB3UIoJNLXVbMR3soxL3Idiv/7yCDme+SQP5e5Gp24GDpaXHxiqbQKBgC9ESSGA
a6OS0mgpEvaCu4dxtorAXUr1LCecaQDGV0rWvvqYQoojGREVgwBnUBACkwtJRe7C
2InSlrvPq0/jU4ap1zYBfHsVRGpwZuOTqUqoAfKddeq1QBeXvTQEPq1bVdudSEZ1
7nJNjyOzCT7kZdidbrBtocYFT/kVxgGE9pezAoGBAKTXN4YplHmESC4h05h4mKKH
xYcPFJV3wlnrKSmbS3FZOLgrTMg415/S+HR5MgX4hmesk9oEtp9E5RLC4X42ntzJ
7nUT7T7c37KqSQDXmjzTLytA7X+zDGwqdE/bDN9Vx6+ESaNaQ229aXP4ywqisuB/
+VZdy8pRndK8SUw+LoSF
-----END PRIVATE KEY-----
"##;
const TEST_JWKS: &str = r#"{
  "keys": [
    {
      "kty": "RSA",
      "use": "sig",
      "alg": "RS256",
      "kid": "test1",
      "n": "lNsW1GGfr2_c7ull5meB3j_wWV4Nx_pjt5glkWXa5MC0C2ha6y39wGhj03v31x8PYx8hHUP3HJ5Qn4x6HWASPmUy5jhdx-5jnMvGN1nt2ZTL0BPTuoCOs0X9XKLoJB9Nj66D4MErcPhfLK2j-hHK2iuyNi3mBxPBeoQ3ie1UyP0Z5NxFapvUqT6ciQ8dGgDPAvzE9etTIsubBn0n_Ot7MlahCGAwH9J3GgsvX3rEZYhTBZ7jmc0VmA757vKQFFAnNyNpfxfoAWhyux-pgY1b9KiuuDJ0Teh4WIBus5FE_vO5ZWkZyolDEARSd075noYpOEjyTDEtoUVxIdbrTaNkLQ",
      "e": "AQAB"
    }
  ]
}"#;

async fn spawn_mock_jwks() -> String {
    async fn jwks() -> Json<Value> {
        Json(serde_json::from_str(TEST_JWKS).unwrap())
    }
    let app = Router::new().route("/jwks.json", axum::routing::get(jwks));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn oidc_id_token_verifies_and_provisions_user() {
    let jwks_url = spawn_mock_jwks().await + "/jwks.json";
    let core = {
        let dir = std::env::temp_dir().join(format!("aihub-oidc-{}", uuid::Uuid::new_v4()));
        let config = Config {
            mode: Mode::Server,
            data_dir: Some(dir.to_string_lossy().to_string()),
            gateway: GatewayConfig {
                host: "127.0.0.1".into(),
                port: 18790,
                request_timeout_ms: 30_000,
            },
            auth: aihub_config::AuthConfig {
                secret_backend: Some("memory".into()),
                oidc_jwks_url: Some(jwks_url.clone()),
                oidc_issuer: Some("https://idp.example.com".into()),
                oidc_audience: Some("aihub".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        aihub_server::bootstrap(config).await.unwrap()
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test1".into());
    let token = encode(
        &header,
        &json!({
            "iss": "https://idp.example.com",
            "aud": "aihub",
            "sub": "employee-77",
            "email": "e77@example.com",
            "exp": chrono::Utc::now().timestamp() + 3600,
        }),
        &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap(),
    )
    .unwrap();

    // 验签 + 映射 + 自动建号
    let user = core
        .state
        .iam
        .identity_from_oidc(&jwks_url, &token, "https://idp.example.com", "aihub")
        .await
        .expect("oidc verification should succeed");
    assert_eq!(user.identity_provider, "oidc");
    assert_eq!(user.email.as_deref(), Some("e77@example.com"));
    let roles = core.state.iam.roles_of(&user.id).await.unwrap();
    assert!(roles.iter().any(|r| r.key == "end_user"));

    // Public OIDC login needs no Admin Token, but does not grant admin permissions.
    let login_request = || {
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/oidc")
            .header("content-type", "application/json")
            .body(Body::from(json!({"idToken": token}).to_string()))
            .unwrap()
    };
    let response = core.router.clone().oneshot(login_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    let session = body["data"]["token"].as_str().unwrap();
    let response = core
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/users")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    core.state
        .repos
        .users
        .set_status(&user.id, "disabled")
        .await
        .unwrap();
    let response = core.router.clone().oneshot(login_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 错误 issuer 拒绝
    let err = core
        .state
        .iam
        .identity_from_oidc(&jwks_url, &token, "https://evil.example.com", "aihub")
        .await;
    assert!(err.is_err(), "issuer mismatch must be rejected");

    // 篡改 token 拒绝
    let tampered = format!("{token}x");
    let err = core
        .state
        .iam
        .identity_from_oidc(&jwks_url, &tampered, "https://idp.example.com", "aihub")
        .await;
    assert!(err.is_err(), "tampered token must be rejected");
}
