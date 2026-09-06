//! Prompt（M8）与 IAM（M10 users/roles）的 SQLite 仓储。

use aihub_domain::error::DomainError;
use aihub_domain::platform::*;
use aihub_domain::prompt::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts};

pub struct SqlitePromptRepository {
    pool: SqlitePool,
}

impl SqlitePromptRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn prompt_from_row(row: &sqlx::sqlite::SqliteRow) -> Prompt {
    Prompt {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        owner_id: row.get("owner_id"),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn version_from_row(row: &sqlx::sqlite::SqliteRow) -> PromptVersion {
    PromptVersion {
        id: row.get("id"),
        prompt_id: row.get("prompt_id"),
        version: row.get::<i64, _>("version") as i32,
        status: PromptVersionStatus::parse(&row.get::<String, _>("status"))
            .unwrap_or(PromptVersionStatus::Draft),
        system_template: row.get("system_template"),
        user_template: row.get("user_template"),
        variables_schema: parse_json(Some(row.get("variables_schema_json"))),
        model_config: parse_json(Some(row.get("model_config_json"))),
        output_schema: row
            .get::<Option<String>, _>("output_schema_json")
            .map(|s| parse_json(Some(s))),
        created_by: row.get("created_by"),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
    }
}

#[async_trait]
impl PromptRepository for SqlitePromptRepository {
    async fn create(&self, prompt: NewPrompt) -> Result<Prompt, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        sqlx::query("INSERT INTO prompts (id, key, name, description, owner_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&prompt.key)
            .bind(&prompt.name)
            .bind(&prompt.description)
            .bind(&prompt.owner_id)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<Prompt, DomainError> {
        sqlx::query("SELECT * FROM prompts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| prompt_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Prompt, id))
    }

    async fn get_by_key(&self, key: &str) -> Result<Prompt, DomainError> {
        sqlx::query("SELECT * FROM prompts WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| prompt_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Prompt, key))
    }

    async fn list(&self) -> Result<Vec<Prompt>, DomainError> {
        let rows = sqlx::query("SELECT * FROM prompts ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        Ok(rows.iter().map(prompt_from_row).collect())
    }

    async fn update(
        &self,
        id: &str,
        name: String,
        description: Option<String>,
    ) -> Result<Prompt, DomainError> {
        sqlx::query("UPDATE prompts SET name=?, description=?, updated_at=? WHERE id=?")
            .bind(&name)
            .bind(&description)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM prompts WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        Ok(())
    }

    async fn create_version(
        &self,
        version: NewPromptVersion,
    ) -> Result<PromptVersion, DomainError> {
        // 版本号自增（prompt_id, version 唯一）
        let row = sqlx::query(
            "SELECT COALESCE(MAX(version), 0) AS v FROM prompt_versions WHERE prompt_id = ?",
        )
        .bind(&version.prompt_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Prompt, e))?;
        let next = row.get::<i64, _>("v") + 1;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO prompt_versions (id, prompt_id, version, status, system_template, user_template, variables_schema_json, model_config_json, output_schema_json, created_by, created_at)
             VALUES (?, ?, ?, 'draft', ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&version.prompt_id)
        .bind(next)
        .bind(&version.system_template)
        .bind(&version.user_template)
        .bind(json_string(&version.variables_schema))
        .bind(json_string(&version.model_config))
        .bind(version.output_schema.as_ref().map(|v| v.to_string()))
        .bind(&version.created_by)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get_version(&id).await
    }

    async fn versions_for(&self, prompt_id: &str) -> Result<Vec<PromptVersion>, DomainError> {
        let rows =
            sqlx::query("SELECT * FROM prompt_versions WHERE prompt_id = ? ORDER BY version DESC")
                .bind(prompt_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Prompt, e))?;
        Ok(rows.iter().map(version_from_row).collect())
    }

    async fn get_version(&self, version_id: &str) -> Result<PromptVersion, DomainError> {
        sqlx::query("SELECT * FROM prompt_versions WHERE id = ?")
            .bind(version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| version_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Prompt, version_id))
    }

    async fn published_version(
        &self,
        prompt_id: &str,
    ) -> Result<Option<PromptVersion>, DomainError> {
        Ok(sqlx::query("SELECT * FROM prompt_versions WHERE prompt_id = ? AND status = 'published' ORDER BY version DESC LIMIT 1")
            .bind(prompt_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| version_from_row(&r)))
    }

    async fn set_version_status(
        &self,
        version_id: &str,
        status: PromptVersionStatus,
    ) -> Result<PromptVersion, DomainError> {
        sqlx::query("UPDATE prompt_versions SET status = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(version_id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get_version(version_id).await
    }
}

pub struct SqliteUserRepository {
    pool: SqlitePool,
}

impl SqliteUserRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn user_from_row(row: &sqlx::sqlite::SqliteRow) -> User {
    User {
        id: row.get("id"),
        external_subject: row.get("external_subject"),
        identity_provider: row.get("identity_provider"),
        username: row.get("username"),
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        password_hash: row.get("password_hash"),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
    }
}

fn role_from_row(row: &sqlx::sqlite::SqliteRow) -> Role {
    Role {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        system_role: row.get::<i64, _>("system_role") != 0,
    }
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn create(&self, user: NewUser) -> Result<User, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, external_subject, identity_provider, username, email, display_name, status, password_hash, metadata_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'active', ?, '{}', ?, ?)",
        )
        .bind(&id)
        .bind(&user.external_subject)
        .bind(&user.identity_provider)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(now_rfc3339())
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::User, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<User, DomainError> {
        sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?
            .map(|r| user_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::User, id))
    }

    async fn get_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        Ok(sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?
            .map(|r| user_from_row(&r)))
    }

    async fn get_by_subject(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<User>, DomainError> {
        Ok(
            sqlx::query("SELECT * FROM users WHERE identity_provider = ? AND external_subject = ?")
                .bind(provider)
                .bind(subject)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::User, e))?
                .map(|r| user_from_row(&r)),
        )
    }

    async fn list(&self) -> Result<Vec<User>, DomainError> {
        let rows = sqlx::query("SELECT * FROM users ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(rows.iter().map(user_from_row).collect())
    }

    async fn set_status(&self, id: &str, status: &str) -> Result<(), DomainError> {
        sqlx::query("UPDATE users SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(())
    }

    async fn assign_role(&self, user_id: &str, role_key: &str) -> Result<(), DomainError> {
        let role = sqlx::query("SELECT id FROM roles WHERE key = ?")
            .bind(role_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::User, role_key))?;
        sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role_id, scope_type, created_at) VALUES (?, ?, 'global', ?)")
            .bind(user_id)
            .bind(role.get::<String, _>("id"))
            .bind(now_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(())
    }

    async fn roles_of(&self, user_id: &str) -> Result<Vec<Role>, DomainError> {
        let rows = sqlx::query(
            "SELECT r.* FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = ?",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(rows.iter().map(role_from_row).collect())
    }

    async fn ensure_role(
        &self,
        key: &str,
        name: &str,
        description: &str,
        permissions: &[&str],
    ) -> Result<Role, DomainError> {
        let existing = sqlx::query("SELECT * FROM roles WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        let role_id = match existing {
            Some(row) => row.get::<String, _>("id"),
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO roles (id, key, name, description, system_role, created_at) VALUES (?, ?, ?, ?, 1, ?)")
                    .bind(&id)
                    .bind(key)
                    .bind(name)
                    .bind(description)
                    .bind(now_rfc3339())
                    .execute(&self.pool)
                    .await
                    .map_err(|e| db_error(DomainResource::User, e))?;
                id
            }
        };
        for permission in permissions {
            let pid = sqlx::query("SELECT id FROM permissions WHERE key = ?")
                .bind(permission)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::User, e))?;
            let pid = match pid {
                Some(row) => row.get::<String, _>("id"),
                None => {
                    let pid = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO permissions (id, key, description) VALUES (?, ?, ?)")
                        .bind(&pid)
                        .bind(permission)
                        .bind(permission)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| db_error(DomainResource::User, e))?;
                    pid
                }
            };
            sqlx::query(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
            )
            .bind(&role_id)
            .bind(&pid)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        }
        let row = sqlx::query("SELECT * FROM roles WHERE id = ?")
            .bind(&role_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(role_from_row(&row))
    }

    async fn list_roles(&self) -> Result<Vec<Role>, DomainError> {
        let rows = sqlx::query("SELECT * FROM roles ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(rows.iter().map(role_from_row).collect())
    }
}

#[allow(dead_code)]
fn unused_now() -> chrono::DateTime<Utc> {
    Utc::now()
}
