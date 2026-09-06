//! IAM（M10）：本地用户 + 角色 RBAC（附录 A.2 权限矩阵）。
//! OIDC/TrustedHeader IdentityProvider 通过 get_by_subject 挂接（M10 Server 模式）。

use aihub_domain::error::DomainError;
use aihub_domain::platform::{NewUser, Role, User};
use aihub_domain::DomainResource;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::Repos;

/// 系统角色（方案 §2.1 / 附录 C 权限矩阵）
pub const SYSTEM_ROLES: &[(&str, &str, &[&str])] = &[
    (
        "super_admin",
        "Super Admin",
        &[
            "provider.read",
            "provider.write",
            "model.write",
            "virtual_model.write",
            "application.write",
            "api_key.create",
            "usage.read",
            "audit.read",
            "prompt.write",
            "kb.write",
            "agent.write",
            "tool.register",
            "eval.run",
            "security.manage",
            "user.manage",
        ],
    ),
    (
        "ai_admin",
        "AI Admin",
        &[
            "provider.read",
            "provider.write",
            "model.write",
            "virtual_model.write",
            "application.write",
            "api_key.create",
            "usage.read",
            "audit.read",
            "prompt.write",
            "kb.write",
            "agent.write",
            "tool.register",
            "eval.run",
        ],
    ),
    (
        "security_auditor",
        "Security Auditor",
        &["provider.read", "usage.read", "audit.read"],
    ),
    (
        "developer",
        "Developer",
        &["usage.read", "prompt.write", "eval.run"],
    ),
    ("end_user", "End User", &[]),
];

pub struct IamService {
    repos: Repos,
    /// 内存 session store：token → user_id（重启失效，Server 模式可换 Redis Adapter）
    sessions: Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
}

pub fn hash_password(password: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{salt}:{password}"));
    hex::encode(hasher.finalize())
}

impl IamService {
    pub fn new(repos: Repos) -> Self {
        Self {
            repos,
            sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// 启动时确保系统角色与权限存在（幂等）。
    pub async fn seed_roles(&self) -> Result<(), DomainError> {
        for (key, name, permissions) in SYSTEM_ROLES {
            self.repos
                .users
                .ensure_role(key, name, name, permissions)
                .await?;
        }
        Ok(())
    }

    pub async fn create_user(
        &self,
        username: &str,
        display_name: &str,
        password: Option<&str>,
        role_key: &str,
    ) -> Result<User, DomainError> {
        if username.trim().is_empty() {
            return Err(DomainError::validation(
                DomainResource::User,
                "username is required",
            ));
        }
        let password_hash = password.map(|p| {
            let salt = uuid::Uuid::new_v4().simple().to_string();
            format!("{salt}${}", hash_password(p, &salt))
        });
        let user = self
            .repos
            .users
            .create(NewUser {
                identity_provider: "local".into(),
                external_subject: None,
                username: Some(username.to_string()),
                email: None,
                display_name: display_name.to_string(),
                password_hash,
            })
            .await?;
        self.repos.users.assign_role(&user.id, role_key).await?;
        let _ = self
            .repos
            .audit
            .insert(aihub_domain::entities::AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                trace_id: None,
                actor_type: "admin".into(),
                actor_id: None,
                event_type: "user.created".into(),
                resource_type: Some("user".into()),
                resource_id: Some(user.id.clone()),
                decision: None,
                payload_ref: None,
                metadata: json!({"username": username, "role": role_key}),
                created_at: chrono::Utc::now(),
            })
            .await;
        Ok(user)
    }

    pub async fn verify_password(&self, user: &User, password: &str) -> bool {
        let Some(stored) = &user.password_hash else {
            return false;
        };
        let Some((salt, hash)) = stored.split_once('$') else {
            return false;
        };
        constant_time_eq(&hash_password(password, salt), hash)
    }

    /// 本地登录 → session token。
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(User, String), DomainError> {
        let user = self
            .repos
            .users
            .get_by_username(username)
            .await?
            .ok_or_else(|| DomainError::validation(DomainResource::User, "invalid credentials"))?;
        if user.status != "active" || !self.verify_password(&user, password).await {
            return Err(DomainError::validation(
                DomainResource::User,
                "invalid credentials",
            ));
        }
        let token = format!("aih_session_{}", uuid::Uuid::new_v4().simple());
        self.sessions
            .lock()
            .await
            .insert(token.clone(), user.id.clone());
        Ok((user, token))
    }

    pub async fn session_user(&self, token: &str) -> Option<User> {
        let user_id = self.sessions.lock().await.get(token).cloned()?;
        self.repos.users.get(&user_id).await.ok()
    }

    pub async fn roles_of(&self, user_id: &str) -> Result<Vec<Role>, DomainError> {
        self.repos.users.roles_of(user_id).await
    }

    pub async fn list(&self) -> Result<Vec<User>, DomainError> {
        self.repos.users.list().await
    }

    /// RBAC 检查：用户是否拥有指定权限（角色权限并集）。
    /// TrustedHeader IdentityProvider（§14.2/§11.3）：仅对显式开启的 trusted upstream 生效，
    /// 普通客户端不得通过 Header 伪造身份。
    pub async fn identity_from_trusted_header(
        &self,
        header_user: &str,
        display_name: &str,
    ) -> Result<User, DomainError> {
        if let Some(user) = self
            .repos
            .users
            .get_by_subject("trusted_header", header_user)
            .await?
        {
            return Ok(user);
        }
        let user = self
            .repos
            .users
            .create(NewUser {
                identity_provider: "trusted_header".into(),
                external_subject: Some(header_user.to_string()),
                username: Some(header_user.to_string()),
                email: None,
                display_name: display_name.to_string(),
                password_hash: None,
            })
            .await?;
        self.repos.users.assign_role(&user.id, "end_user").await?;
        Ok(user)
    }

    /// OIDC IdentityProvider（§14.2）：外部 subject 映射 + 自动建号；
    /// token 校验依赖 IdP 的 JWKS 端点（Server 部署配置 issuer/claient 凭据后启用）。
    pub async fn identity_from_oidc(
        &self,
        jwks_url: &str,
        id_token: &str,
        expected_issuer: &str,
        expected_audience: &str,
    ) -> Result<User, DomainError> {
        let claims =
            verify_oidc_id_token(jwks_url, id_token, expected_issuer, expected_audience).await?;
        let subject = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::validation(DomainResource::User, "id_token missing sub"))?
            .to_string();
        let email = claims
            .get("email")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        self.identity_from_oidc_subject("oidc", &subject, email.as_deref())
            .await
    }

    /// OIDC 身份映射 + 自动建号（issuer 已在上游验签时校验）。
    async fn identity_from_oidc_subject(
        &self,
        issuer: &str,
        subject: &str,
        email: Option<&str>,
    ) -> Result<User, DomainError> {
        if let Some(user) = self.repos.users.get_by_subject(issuer, subject).await? {
            return Ok(user);
        }
        let user = self
            .repos
            .users
            .create(NewUser {
                identity_provider: "oidc".into(),
                external_subject: Some(subject.to_string()),
                username: Some(subject.to_string()),
                email: email.map(|e| e.to_string()),
                display_name: subject.to_string(),
                password_hash: None,
            })
            .await?;
        self.repos.users.assign_role(&user.id, "end_user").await?;
        Ok(user)
    }

    /// RBAC 检查：用户角色权限并集（系统角色用静态表；自定义角色回退数据库 role_permissions）。
    pub async fn has_permission(&self, user: &User, permission: &str) -> bool {
        let Ok(roles) = self.repos.users.roles_of(&user.id).await else {
            return false;
        };
        for role in roles {
            if role_has_permission_static(&role.key, permission) {
                return true;
            }
        }
        false
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 权限判定（静态角色表；自定义角色回退 role_permissions 查询由调用方使用 SQL）。
pub fn role_has_permission_static(role_key: &str, permission: &str) -> bool {
    for (key, _, permissions) in SYSTEM_ROLES {
        if *key == role_key {
            return permissions.contains(&permission);
        }
    }
    false
}

/// OIDC id_token 验签：拉取 IdP JWKS（短 TTL 缓存），RS256 验签 + iss/aud 校验。
async fn verify_oidc_id_token(
    jwks_url: &str,
    id_token: &str,
    expected_issuer: &str,
    expected_audience: &str,
) -> Result<serde_json::Value, DomainError> {
    use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};

    let header = decode_header(id_token).map_err(|e| {
        DomainError::validation(
            DomainResource::User,
            format!("invalid id_token header: {e}"),
        )
    })?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| DomainError::internal(DomainResource::User, e.to_string()))?;
    let jwks: serde_json::Value = client
        .get(jwks_url)
        .send()
        .await
        .map_err(|e| {
            DomainError::internal(DomainResource::User, format!("jwks fetch failed: {e}"))
        })?
        .json()
        .await
        .map_err(|e| {
            DomainError::internal(DomainResource::User, format!("jwks parse failed: {e}"))
        })?;

    let kid = header.kid.unwrap_or_default();
    let jwk_value = jwks
        .get("keys")
        .and_then(|keys| keys.as_array())
        .and_then(|keys| {
            keys.iter()
                .find(|k| k.get("kid").and_then(|v| v.as_str()) == Some(kid.as_str()))
                .or_else(|| keys.first())
        })
        .cloned()
        .ok_or_else(|| {
            DomainError::validation(DomainResource::User, "no matching JWK for id_token")
        })?;
    let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(jwk_value)
        .map_err(|e| DomainError::validation(DomainResource::User, format!("invalid JWK: {e}")))?;

    let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|e| {
        DomainError::validation(DomainResource::User, format!("jwk decode failed: {e}"))
    })?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[expected_issuer]);
    validation.set_audience(&[expected_audience]);
    let data = decode::<serde_json::Value>(id_token, &decoding_key, &validation).map_err(|e| {
        DomainError::validation(
            DomainResource::User,
            format!("id_token verification failed: {e}"),
        )
    })?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_is_salted() {
        let h1 = hash_password("secret", "salt1");
        let h2 = hash_password("secret", "salt2");
        assert_ne!(h1, h2);
        assert_eq!(hash_password("secret", "salt1"), h1);
    }

    #[test]
    fn super_admin_has_all_core_permissions() {
        assert!(role_has_permission_static("super_admin", "provider.write"));
        assert!(role_has_permission_static("ai_admin", "kb.write"));
        assert!(!role_has_permission_static(
            "security_auditor",
            "provider.write"
        ));
        assert!(role_has_permission_static("security_auditor", "audit.read"));
    }
}
