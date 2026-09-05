use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts};

pub struct SqliteAuditRepository {
    pool: SqlitePool,
}

impl SqliteAuditRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn event_from_row(row: &sqlx::sqlite::SqliteRow) -> AuditEvent {
    AuditEvent {
        id: row.get("id"),
        trace_id: row.get("trace_id"),
        actor_type: row.get("actor_type"),
        actor_id: row.get("actor_id"),
        event_type: row.get("event_type"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        decision: row.get("decision"),
        payload_ref: row.get("payload_ref"),
        metadata: parse_json(Some(row.get("metadata_json"))),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
    }
}

#[async_trait]
impl AuditRepository for SqliteAuditRepository {
    async fn insert(&self, event: AuditEvent) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO audit_events (id, trace_id, actor_type, actor_id, event_type, resource_type, resource_id, decision, payload_ref, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&event.trace_id)
        .bind(&event.actor_type)
        .bind(&event.actor_id)
        .bind(&event.event_type)
        .bind(&event.resource_type)
        .bind(&event.resource_id)
        .bind(&event.decision)
        .bind(&event.payload_ref)
        .bind(json_string(&event.metadata))
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn list(&self, filter: &AuditFilter) -> Result<(Vec<AuditEvent>, u64), DomainError> {
        let mut where_clause = String::new();
        let mut binds: Vec<String> = Vec::new();
        let mut push = |clause: &str| {
            if where_clause.is_empty() {
                where_clause.push_str(" WHERE ");
            } else {
                where_clause.push_str(" AND ");
            }
            where_clause.push_str(clause);
        };
        if let Some(v) = &filter.event_type {
            push("event_type = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &filter.resource_type {
            push("resource_type = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &filter.actor_id {
            push("actor_id = ?");
            binds.push(v.clone());
        }
        if let Some(v) = &filter.trace_id {
            push("trace_id = ?");
            binds.push(v.clone());
        }

        let count_sql = format!("SELECT COUNT(*) AS c FROM audit_events{where_clause}");
        let mut count_query = sqlx::query(&count_sql);
        for b in &binds {
            count_query = count_query.bind(b);
        }
        let total = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?
            .get::<i64, _>("c");

        let page_size = filter.page_size.clamp(1, 200);
        let offset = filter.page.saturating_sub(1) * page_size;
        let list_sql = format!(
            "SELECT * FROM audit_events{where_clause} ORDER BY created_at DESC LIMIT {page_size} OFFSET {offset}"
        );
        let mut list_query = sqlx::query(&list_sql);
        for b in &binds {
            list_query = list_query.bind(b);
        }
        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok((rows.iter().map(event_from_row).collect(), total as u64))
    }
}
