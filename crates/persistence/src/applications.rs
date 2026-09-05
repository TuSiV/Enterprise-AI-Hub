use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts, parse_vec, ts_to_string};

pub struct SqliteApplicationRepository {
    pool: SqlitePool,
}

impl SqliteApplicationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn app_from_row(row: &sqlx::sqlite::SqliteRow) -> Application {
    Application {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        status: row.get("status"),
        allowed_virtual_models: parse_vec(Some(row.get("allowed_virtual_models_json"))),
        allow_direct_models: row.get::<i64, _>("allow_direct_models") != 0,
        monthly_budget_microunits: row.get("monthly_budget_microunits"),
        metadata: parse_json(Some(row.get("metadata_json"))),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn key_from_row(row: &sqlx::sqlite::SqliteRow) -> ApiKey {
    ApiKey {
        id: row.get("id"),
        application_id: row.get("application_id"),
        name: row.get("name"),
        prefix: row.get("prefix"),
        secret_hash: row.get("secret_hash"),
        scopes: parse_vec(Some(row.get("scopes_json"))),
        expires_at: parse_ts(row.get("expires_at")),
        last_used_at: parse_ts(row.get("last_used_at")),
        revoked_at: parse_ts(row.get("revoked_at")),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
    }
}

#[async_trait]
impl ApplicationRepository for SqliteApplicationRepository {
    async fn create(&self, app: NewApplication) -> Result<Application, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO applications (id, key, name, status, allowed_virtual_models_json, allow_direct_models, monthly_budget_microunits, metadata_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&app.key)
        .bind(&app.name)
        .bind(&app.status)
        .bind(json_string(&serde_json::to_value(&app.allowed_virtual_models).unwrap_or_default()))
        .bind(app.allow_direct_models as i64)
        .bind(app.monthly_budget_microunits)
        .bind(json_string(&app.metadata))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Application, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<Application, DomainError> {
        let row = sqlx::query("SELECT * FROM applications WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Application, id))?;
        Ok(app_from_row(&row))
    }

    async fn get_by_key(&self, key: &str) -> Result<Application, DomainError> {
        let row = sqlx::query("SELECT * FROM applications WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Application, key))?;
        Ok(app_from_row(&row))
    }

    async fn list(&self) -> Result<Vec<Application>, DomainError> {
        let rows = sqlx::query("SELECT * FROM applications ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?;
        Ok(rows.iter().map(app_from_row).collect())
    }

    async fn update(&self, id: &str, update: ApplicationUpdate) -> Result<Application, DomainError> {
        let existing = self.get(id).await?;
        let name = update.name.unwrap_or(existing.name);
        let status = update.status.unwrap_or(existing.status);
        let allowed = update.allowed_virtual_models.unwrap_or(existing.allowed_virtual_models);
        let allow_direct = update.allow_direct_models.unwrap_or(existing.allow_direct_models);
        let budget = update.monthly_budget_microunits.unwrap_or(existing.monthly_budget_microunits);
        let metadata = update.metadata.unwrap_or(existing.metadata);
        sqlx::query(
            "UPDATE applications SET name=?, status=?, allowed_virtual_models_json=?, allow_direct_models=?, monthly_budget_microunits=?, metadata_json=?, updated_at=? WHERE id=?",
        )
        .bind(&name)
        .bind(&status)
        .bind(json_string(&serde_json::to_value(&allowed).unwrap_or_default()))
        .bind(allow_direct as i64)
        .bind(budget)
        .bind(json_string(&metadata))
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Application, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM applications WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?;
        Ok(())
    }
}

pub struct SqliteApiKeyRepository {
    pool: SqlitePool,
}

impl SqliteApiKeyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for SqliteApiKeyRepository {
    async fn create(&self, key: NewApiKey) -> Result<ApiKey, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO api_keys (id, application_id, name, prefix, secret_hash, scopes_json, expires_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&key.application_id)
        .bind(&key.name)
        .bind(&key.prefix)
        .bind(&key.secret_hash)
        .bind(json_string(&serde_json::to_value(&key.scopes).unwrap_or_default()))
        .bind(key.expires_at.map(ts_to_string))
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<ApiKey, DomainError> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::ApiKey, id))?;
        Ok(key_from_row(&row))
    }

    async fn get_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>, DomainError> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE prefix = ? AND revoked_at IS NULL")
            .bind(prefix)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(row.map(|r| key_from_row(&r)))
    }

    async fn list_by_application(&self, application_id: &str) -> Result<Vec<ApiKey>, DomainError> {
        let rows = sqlx::query("SELECT * FROM api_keys WHERE application_id = ? ORDER BY created_at DESC")
            .bind(application_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(rows.iter().map(key_from_row).collect())
    }

    async fn revoke(&self, id: &str, at: chrono::DateTime<Utc>) -> Result<(), DomainError> {
        sqlx::query("UPDATE api_keys SET revoked_at = ? WHERE id = ?")
            .bind(ts_to_string(at))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(())
    }

    async fn touch_last_used(&self, id: &str, at: chrono::DateTime<Utc>) -> Result<(), DomainError> {
        sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
            .bind(ts_to_string(at))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(())
    }

    async fn count_by_application(&self, application_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM api_keys WHERE application_id = ?")
            .bind(application_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(row.get::<i64, _>("c"))
    }
}

pub struct SqliteQuotaRepository {
    pool: SqlitePool,
}

impl SqliteQuotaRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn quota_from_row(row: &sqlx::sqlite::SqliteRow) -> QuotaPolicy {
    QuotaPolicy {
        id: row.get("id"),
        subject_type: row.get("subject_type"),
        subject_id: row.get("subject_id"),
        rpm: row.get("rpm"),
        tpm: row.get("tpm"),
        daily_requests: row.get("daily_requests"),
        monthly_tokens: row.get("monthly_tokens"),
        monthly_cost_microunits: row.get("monthly_cost_microunits"),
        exceed_action: row.get("exceed_action"),
        fallback_virtual_model_id: row.get("fallback_virtual_model_id"),
        enabled: row.get::<i64, _>("enabled") != 0,
    }
}

#[async_trait]
impl QuotaRepository for SqliteQuotaRepository {
    async fn upsert_for_subject(&self, subject_type: &str, subject_id: &str, values: QuotaValues) -> Result<QuotaPolicy, DomainError> {
        sqlx::query(
            "INSERT INTO quota_policies (id, subject_type, subject_id, rpm, tpm, daily_requests, monthly_tokens, monthly_cost_microunits, exceed_action, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)
             ON CONFLICT(subject_type, subject_id) DO UPDATE SET
               rpm=excluded.rpm, tpm=excluded.tpm, daily_requests=excluded.daily_requests,
               monthly_tokens=excluded.monthly_tokens, monthly_cost_microunits=excluded.monthly_cost_microunits,
               exceed_action=excluded.exceed_action, updated_at=excluded.updated_at",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(subject_type)
        .bind(subject_id)
        .bind(values.rpm)
        .bind(values.tpm)
        .bind(values.daily_requests)
        .bind(values.monthly_tokens)
        .bind(values.monthly_cost_microunits)
        .bind(&values.exceed_action)
        .bind(now_rfc3339())
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Application, e))?;
        self.get_for_subject(subject_type, subject_id)
            .await?
            .ok_or_else(|| DomainError::internal(DomainResource::Application, "quota policy vanished"))
    }

    async fn get_for_subject(&self, subject_type: &str, subject_id: &str) -> Result<Option<QuotaPolicy>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM quota_policies WHERE subject_type = ? AND subject_id = ? AND enabled = 1",
        )
        .bind(subject_type)
        .bind(subject_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Application, e))?;
        Ok(row.map(|r| quota_from_row(&r)))
    }
}
