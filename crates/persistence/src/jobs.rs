//! Runtime Jobs（M11，附录 A.7 / §21.6）：文档解析、Embedding 等 Job 的持久化队列。
//! Desktop/单实例 Server：Tokio 后台 worker + job 表；多实例时接 Redis/NATS（Stage F）。

use aihub_domain::error::DomainError;
use aihub_domain::platform::{JobRepository, RuntimeJob};
use aihub_domain::DomainResource;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339};

pub struct SqliteJobRepository {
    pool: SqlitePool,
}

impl SqliteJobRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn job_from_row(row: &sqlx::sqlite::SqliteRow) -> RuntimeJob {
    RuntimeJob {
        id: row.get("id"),
        job_type: row.get("job_type"),
        status: row.get("status"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        payload: crate::parse_json(Some(row.get("payload_json"))),
        attempt: row.get::<i64, _>("attempt") as i32,
        max_attempts: row.get::<i64, _>("max_attempts") as i32,
        last_error: row.get("last_error"),
    }
}

#[async_trait]
#[async_trait]
impl JobRepository for SqliteJobRepository {
    async fn enqueue(
        &self,
        job_type: &str,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        payload: &Value,
    ) -> Result<String, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO runtime_jobs (id, job_type, status, resource_type, resource_id, payload_json, attempt, max_attempts, available_at, created_at) VALUES (?, ?, 'pending', ?, ?, ?, 0, 3, ?, ?)")
            .bind(&id)
            .bind(job_type)
            .bind(resource_type)
            .bind(resource_id)
            .bind(json_string(payload))
            .bind(now_rfc3339())
            .bind(now_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(id)
    }

    async fn claim_due(&self) -> Result<Option<RuntimeJob>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM runtime_jobs WHERE status = 'pending' AND available_at <= ? ORDER BY created_at LIMIT 1",
        )
        .bind(now_rfc3339())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        let Some(row) = row else { return Ok(None) };
        let job = job_from_row(&row);
        sqlx::query("UPDATE runtime_jobs SET status = 'running', attempt = attempt + 1, started_at = ? WHERE id = ?")
            .bind(now_rfc3339())
            .bind(&job.id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(Some(job))
    }

    async fn finish(
        &self,
        id: &str,
        status: &str,
        error: Option<String>,
    ) -> Result<(), DomainError> {
        // 失败且未超重试上限 → 回到 pending 延迟重试
        if status == "failed" {
            let updated = sqlx::query(
                "UPDATE runtime_jobs SET status = CASE WHEN attempt < max_attempts THEN 'pending' ELSE 'failed' END, last_error = ?, completed_at = ?, available_at = ? WHERE id = ?",
            )
            .bind(&error)
            .bind(now_rfc3339())
            .bind(chrono::Utc::now() + chrono::Duration::seconds(30))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
            let _ = updated;
            return Ok(());
        }
        sqlx::query("UPDATE runtime_jobs SET status = ?, completed_at = ? WHERE id = ?")
            .bind(status)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn list(&self, limit: u64) -> Result<Vec<RuntimeJob>, DomainError> {
        let rows = sqlx::query("SELECT * FROM runtime_jobs ORDER BY created_at DESC LIMIT ?")
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows.iter().map(job_from_row).collect())
    }

    async fn requeue(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("UPDATE runtime_jobs SET status = 'pending', available_at = ?, attempt = 0 WHERE id = ?")
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }
}
