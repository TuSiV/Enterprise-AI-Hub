use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts, ts_to_string};

pub struct SqliteProviderRepository {
    pool: SqlitePool,
}

impl SqliteProviderRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn provider_from_row(row: &sqlx::sqlite::SqliteRow) -> Provider {
    Provider {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        kind: ProviderKind::parse(&row.get::<String, _>("kind")).unwrap_or(ProviderKind::OpenAICompatible),
        base_url: row.get("base_url"),
        credential_ref: row.get("credential_ref"),
        credential_configured: row.get::<i64, _>("credential_configured") != 0,
        proxy_url: row.get("proxy_url"),
        timeout_ms: row.get::<i64, _>("timeout_ms"),
        max_retries: row.get::<i64, _>("max_retries") as i32,
        enabled: row.get::<i64, _>("enabled") != 0,
        status: row.get("status"),
        health: row.get("health"),
        last_health_check_at: parse_ts(row.get("last_health_check_at")),
        config: parse_json(Some(row.get("config_json"))),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

#[async_trait]
impl ProviderRepository for SqliteProviderRepository {
    async fn create(&self, provider: NewProvider) -> Result<Provider, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let credential_ref = format!("provider/{id}/api_key");
        sqlx::query(
            "INSERT INTO providers (id, key, name, kind, base_url, credential_ref, credential_configured, proxy_url, timeout_ms, max_retries, enabled, status, health, config_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 'unknown', ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&provider.key)
        .bind(&provider.name)
        .bind(provider.kind.as_str())
        .bind(&provider.base_url)
        .bind(&credential_ref)
        .bind(&provider.proxy_url)
        .bind(provider.timeout_ms)
        .bind(provider.max_retries as i64)
        .bind(provider.enabled as i64)
        .bind(&provider.status)
        .bind(json_string(&provider.config))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Provider, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<Provider, DomainError> {
        let row = sqlx::query("SELECT * FROM providers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Provider, id))?;
        Ok(provider_from_row(&row))
    }

    async fn get_by_key(&self, key: &str) -> Result<Provider, DomainError> {
        let row = sqlx::query("SELECT * FROM providers WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Provider, key))?;
        Ok(provider_from_row(&row))
    }

    async fn list(&self, enabled_only: bool) -> Result<Vec<Provider>, DomainError> {
        let sql = if enabled_only {
            "SELECT * FROM providers WHERE enabled = 1 ORDER BY created_at"
        } else {
            "SELECT * FROM providers ORDER BY created_at"
        };
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(rows.iter().map(provider_from_row).collect())
    }

    async fn update(&self, id: &str, update: ProviderUpdate) -> Result<Provider, DomainError> {
        let existing = self.get(id).await?;
        let credential_configured = update.credential_configured.unwrap_or(existing.credential_configured);
        let name = update.name.unwrap_or(existing.name);
        let base_url = update.base_url.unwrap_or(existing.base_url);
        let proxy_url = update
            .proxy_url
            .unwrap_or(existing.proxy_url);
        let timeout_ms = update.timeout_ms.unwrap_or(existing.timeout_ms);
        let max_retries = update.max_retries.unwrap_or(existing.max_retries);
        let enabled = update.enabled.unwrap_or(existing.enabled);
        let status = update.status.unwrap_or(existing.status);
        let config = update.config.unwrap_or(existing.config);
        let credential_ref = update
            .credential_ref
            .unwrap_or(existing.credential_ref.unwrap_or_else(|| format!("provider/{id}/api_key")));
        sqlx::query(
            "UPDATE providers SET name=?, base_url=?, credential_configured=?, proxy_url=?, timeout_ms=?, max_retries=?, enabled=?, status=?, config_json=?, credential_ref=?, updated_at=? WHERE id=?",
        )
        .bind(&name)
        .bind(&base_url)
        .bind(credential_configured as i64)
        .bind(&proxy_url)
        .bind(timeout_ms)
        .bind(max_retries as i64)
        .bind(enabled as i64)
        .bind(&status)
        .bind(json_string(&config))
        .bind(&credential_ref)
        .bind(ts_to_string(Utc::now()))
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Provider, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM providers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(())
    }

    async fn set_health(&self, id: &str, health: &str, checked_at: chrono::DateTime<Utc>) -> Result<(), DomainError> {
        sqlx::query("UPDATE providers SET health=?, last_health_check_at=?, updated_at=? WHERE id=?")
            .bind(health)
            .bind(ts_to_string(checked_at))
            .bind(ts_to_string(Utc::now()))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(())
    }
}

pub struct SqliteProviderHealthRepository {
    pool: SqlitePool,
}

impl SqliteProviderHealthRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProviderHealthRepository for SqliteProviderHealthRepository {
    async fn insert_sample(&self, sample: NewHealthSample) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO provider_health_samples (id, provider_id, model_id, status, latency_ms, http_status, error_category, checked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&sample.provider_id)
        .bind(&sample.model_id)
        .bind(&sample.status)
        .bind(sample.latency_ms)
        .bind(sample.http_status)
        .bind(sample.error_category.map(|c| c.to_string()))
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(())
    }

    async fn latest_per_provider(&self) -> Result<Vec<ProviderHealthSample>, DomainError> {
        let rows = sqlx::query(
            "SELECT h.* FROM provider_health_samples h
             JOIN (SELECT provider_id, MAX(checked_at) AS latest FROM provider_health_samples GROUP BY provider_id) t
             ON h.provider_id = t.provider_id AND h.checked_at = t.latest",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(rows
            .iter()
            .map(|row| ProviderHealthSample {
                id: row.get("id"),
                provider_id: row.get("provider_id"),
                model_id: row.get("model_id"),
                status: row.get("status"),
                latency_ms: row.get("latency_ms"),
                http_status: row.get("http_status"),
                error_category: row.get("error_category"),
                checked_at: parse_ts(Some(row.get("checked_at"))).unwrap_or_default(),
            })
            .collect())
    }
}
