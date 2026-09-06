//! SQLite Persistence Adapter（方案 §21.1）。PostgreSQL Adapter 在 M10 阶段
//! 按相同 Port 实现；本 crate 只依赖 domain 定义的 trait。

mod applications;
mod audit;
mod jobs;
mod models;
#[cfg(feature = "postgres")]
pub mod pg_core;
#[cfg(feature = "postgres")]
pub mod pg_platform;
mod platform_store;
mod prompt_iam;
mod providers;
mod requests;
mod virtual_models;

pub use applications::{
    SqliteApiKeyRepository, SqliteApplicationRepository, SqliteQuotaRepository,
};
pub use audit::SqliteAuditRepository;
pub use jobs::SqliteJobRepository;
pub use models::SqliteModelRepository;
pub use platform_store::{
    SqliteAgentRepository, SqliteEvalRepository, SqliteKnowledgeRepository,
    SqliteMcpServerRepository, SqlitePolicyRepository, SqliteToolRepository,
};
pub use prompt_iam::{SqlitePromptRepository, SqliteUserRepository};
pub use providers::{SqliteProviderHealthRepository, SqliteProviderRepository};
pub use requests::SqliteRequestRepository;
pub use virtual_models::SqliteVirtualModelRepository;

// usage 聚合与 request 记录同库实现，复用连接池
pub use requests::SqliteUsageRepository;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn open_sqlite(path: &std::path::Path) -> Result<SqlitePool, PersistenceError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let url = format!("sqlite://{}", path.display());
    let options = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;
    run_migrations(&pool).await?;
    Ok(pool)
}

/// 迁移失败不应继续进入正常服务（方案 §23.1）。
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), PersistenceError> {
    sqlx::migrate!("migrations/sqlite").run(pool).await?;
    Ok(())
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

pub(crate) fn ts_to_string(ts: chrono::DateTime<chrono::Utc>) -> String {
    ts.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

pub(crate) fn parse_ts(value: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    value.and_then(|v| {
        chrono::DateTime::parse_from_rfc3339(&v)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc))
    })
}

pub(crate) fn parse_json(value: Option<String>) -> serde_json::Value {
    value
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(serde_json::Value::Null)
}

pub(crate) fn json_string(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn parse_vec(value: Option<String>) -> Vec<String> {
    value
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

/// 仓储层错误 → DomainError 的统一出口。
pub(crate) fn db_error(
    resource: aihub_domain::DomainResource,
    err: sqlx::Error,
) -> aihub_domain::DomainError {
    match &err {
        sqlx::Error::RowNotFound => aihub_domain::DomainError::not_found(resource, "row"),
        sqlx::Error::Database(db_err) => {
            if db_err.is_unique_violation() {
                aihub_domain::DomainError::duplicate(resource, "key")
            } else {
                aihub_domain::DomainError::internal(resource, format!("db error: {db_err}"))
            }
        }
        other => aihub_domain::DomainError::internal(resource, format!("db error: {other}")),
    }
}

#[cfg(feature = "postgres")]
pub use pg_core::open_postgres;
