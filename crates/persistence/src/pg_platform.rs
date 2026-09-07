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

//! PostgreSQL：Request/Usage/Audit/Prompt/User 仓储（pg_core 的延续）。

use super::pg_core::db_error;
use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::platform::{NewUser, Role, User, UserRepository};
use aihub_domain::prompt::NewPromptVersion;
use aihub_domain::repos::*;
use aihub_domain::{DomainErrorCode, DomainResource};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

type Result<T> = std::result::Result<T, DomainError>;

fn prefix_and(where_clause: &str) -> &'static str {
    if where_clause.is_empty() {
        " WHERE "
    } else {
        " AND "
    }
}

// ================= Request / Usage / Cost =================

pub struct PgRequestRepository {
    pool: PgPool,
}

impl PgRequestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn request_from_row(row: &sqlx::postgres::PgRow) -> AiRequest {
    AiRequest {
        id: row.get("id"),
        trace_id: row.get("trace_id"),
        application_id: row.get("application_id"),
        user_id: row.get("user_id"),
        api_key_id: row.get("api_key_id"),
        endpoint: row.get("endpoint"),
        requested_model: row.get("requested_model"),
        resolved_model_id: row.get("resolved_model_id"),
        resolved_model_key: row.get("resolved_model_key"),
        provider_id: row.get("provider_id"),
        status: match row.get::<String, _>("status").as_str() {
            "completed" => RequestStatus::Completed,
            "failed" => RequestStatus::Failed,
            "client_cancelled" => RequestStatus::ClientCancelled,
            "timeout" => RequestStatus::Timeout,
            "routing" => RequestStatus::Routing,
            "running" => RequestStatus::Running,
            _ => RequestStatus::Accepted,
        },
        http_status: row.get::<Option<i32>, _>("http_status").map(|v| v as i64),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        ttft_ms: row.get("ttft_ms"),
        latency_ms: row.get("latency_ms"),
        retry_count: row.get("retry_count"),
        cache_status: row.get("cache_status"),
        error_code: row.get("error_code"),
        error_message_safe: row.get("error_message_safe"),
        metadata: row.get::<Value, _>("metadata_json"),
    }
}

#[async_trait]
impl RequestRepository for PgRequestRepository {
    async fn create(&self, r: NewAiRequest) -> Result<AiRequest> {
        sqlx::query("INSERT INTO ai_requests (id, trace_id, application_id, user_id, api_key_id, endpoint, requested_model, status, started_at, metadata_json) VALUES ($1,$2,$3,$4,$5,$6,$7,'accepted',$8,$9)")
            .bind(&r.id).bind(&r.trace_id).bind(&r.application_id).bind(&r.user_id)
            .bind(&r.api_key_id).bind(&r.endpoint).bind(&r.requested_model)
            .bind(r.started_at).bind(&r.metadata)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        self.get(&r.id).await
    }

    async fn finish(&self, id: &str, f: RequestFinish) -> Result<()> {
        sqlx::query("UPDATE ai_requests SET status=$1, http_status=$2, completed_at=$3, ttft_ms=$4, latency_ms=$5, retry_count=$6, error_code=$7, error_message_safe=$8, resolved_model_id=$9, resolved_model_key=$10, provider_id=$11 WHERE id=$12")
            .bind(f.status.as_str()).bind(f.http_status).bind(f.completed_at)
            .bind(f.ttft_ms).bind(f.latency_ms).bind(f.retry_count)
            .bind(&f.error_code).bind(&f.error_message_safe)
            .bind(&f.resolved_model_id).bind(&f.resolved_model_key).bind(&f.provider_id)
            .bind(id)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<AiRequest> {
        let row = sqlx::query("SELECT * FROM ai_requests WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Request, id))?;
        Ok(request_from_row(&row))
    }

    async fn list(&self, filter: &RequestFilter) -> Result<(Vec<AiRequest>, u64)> {
        let mut where_clause = String::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &filter.application_id {
            binds.push(app.clone());
            where_clause.push_str(&format!(
                "{}application_id = ${}",
                prefix_and(&where_clause),
                binds.len()
            ));
        }
        if let Some(status) = &filter.status {
            binds.push(status.clone());
            where_clause.push_str(&format!(
                "{}status = ${}",
                prefix_and(&where_clause),
                binds.len()
            ));
        }
        if let Some(model) = &filter.model {
            binds.push(model.clone());
            let n = binds.len();
            where_clause.push_str(&format!(
                "{}(resolved_model_key = ${n} OR requested_model = ${n})",
                prefix_and(&where_clause)
            ));
        }
        if let Some(from) = filter.from {
            binds.push(from.to_rfc3339());
            where_clause.push_str(&format!(
                "{}started_at >= ${}::timestamptz",
                prefix_and(&where_clause),
                binds.len()
            ));
        }
        if let Some(to) = filter.to {
            binds.push(to.to_rfc3339());
            where_clause.push_str(&format!(
                "{}started_at <= ${}::timestamptz",
                prefix_and(&where_clause),
                binds.len()
            ));
        }

        let count_sql = format!("SELECT COUNT(*)::bigint AS c FROM ai_requests{where_clause}");
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
        let list_sql = format!("SELECT * FROM ai_requests{where_clause} ORDER BY started_at DESC LIMIT {page_size} OFFSET {offset}");
        let mut list_query = sqlx::query(&list_sql);
        for b in &binds {
            list_query = list_query.bind(b);
        }
        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok((rows.iter().map(request_from_row).collect(), total as u64))
    }

    async fn insert_usage(&self, u: UsageRecord) -> Result<()> {
        sqlx::query("INSERT INTO usage_records (id, request_id, input_tokens, output_tokens, cached_input_tokens, reasoning_tokens, total_tokens, usage_source, raw_usage_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&u.id).bind(&u.request_id).bind(u.input_tokens).bind(u.output_tokens)
            .bind(u.cached_input_tokens).bind(u.reasoning_tokens).bind(u.total_tokens)
            .bind(&u.usage_source).bind(&u.raw_usage).bind(u.created_at)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn insert_cost(&self, c: CostRecord) -> Result<()> {
        sqlx::query("INSERT INTO cost_records (id, request_id, currency, input_cost_microunits, output_cost_microunits, cache_cost_microunits, reasoning_cost_microunits, total_cost_microunits, pricing_snapshot_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&c.id).bind(&c.request_id).bind(&c.currency)
            .bind(c.input_cost_microunits).bind(c.output_cost_microunits)
            .bind(c.cache_cost_microunits).bind(c.reasoning_cost_microunits)
            .bind(c.total_cost_microunits).bind(&c.pricing_snapshot).bind(c.created_at)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn usage_for_request(&self, request_id: &str) -> Result<Option<UsageRecord>> {
        Ok(
            sqlx::query("SELECT * FROM usage_records WHERE request_id = $1")
                .bind(request_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Request, e))?
                .map(|r| UsageRecord {
                    id: r.get("id"),
                    request_id: r.get("request_id"),
                    input_tokens: r.get("input_tokens"),
                    output_tokens: r.get("output_tokens"),
                    cached_input_tokens: r.get("cached_input_tokens"),
                    reasoning_tokens: r.get("reasoning_tokens"),
                    total_tokens: r.get("total_tokens"),
                    usage_source: r.get("usage_source"),
                    raw_usage: r.get::<Value, _>("raw_usage_json"),
                    created_at: r.get("created_at"),
                }),
        )
    }

    async fn cost_for_request(&self, request_id: &str) -> Result<Option<CostRecord>> {
        Ok(
            sqlx::query("SELECT * FROM cost_records WHERE request_id = $1")
                .bind(request_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Request, e))?
                .map(|r| CostRecord {
                    id: r.get("id"),
                    request_id: r.get("request_id"),
                    currency: r.get("currency"),
                    input_cost_microunits: r.get("input_cost_microunits"),
                    output_cost_microunits: r.get("output_cost_microunits"),
                    cache_cost_microunits: r.get("cache_cost_microunits"),
                    reasoning_cost_microunits: r.get("reasoning_cost_microunits"),
                    total_cost_microunits: r.get("total_cost_microunits"),
                    pricing_snapshot: r.get::<Value, _>("pricing_snapshot_json"),
                    created_at: r.get("created_at"),
                }),
        )
    }

    async fn update_resolved(
        &self,
        id: &str,
        model_id: &str,
        model_key: &str,
        provider_id: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE ai_requests SET resolved_model_id=$1, resolved_model_key=$2, provider_id=$3, status='running' WHERE id=$4")
            .bind(model_id).bind(model_key).bind(provider_id).bind(id)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }
}

pub struct PgUsageRepository {
    pool: PgPool,
}

impl PgUsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn agg_from_row(row: &sqlx::postgres::PgRow) -> UsageAggregates {
    UsageAggregates {
        requests: row.get("requests"),
        success_requests: row.get("success_requests"),
        failed_requests: row.get("failed_requests"),
        input_tokens: row.get("input_tokens"),
        output_tokens: row.get("output_tokens"),
        cached_input_tokens: row.get("cached_input_tokens"),
        total_tokens: row.get("total_tokens"),
        cost_microunits: row.get("cost_microunits"),
        p50_latency_ms: None,
        p95_latency_ms: None,
        avg_ttft_ms: row.get("avg_ttft_ms"),
    }
}

const AGG_SELECT: &str = "COUNT(*) AS requests, \
     COALESCE(SUM(CASE WHEN r.status = 'completed' THEN 1 ELSE 0 END),0)::bigint AS success_requests, \
     COALESCE(SUM(CASE WHEN r.status IN ('failed','timeout') THEN 1 ELSE 0 END),0)::bigint AS failed_requests, \
     COALESCE(SUM(u.input_tokens),0)::bigint AS input_tokens, \
     COALESCE(SUM(u.output_tokens),0)::bigint AS output_tokens, \
     COALESCE(SUM(u.cached_input_tokens),0)::bigint AS cached_input_tokens, \
     COALESCE(SUM(u.total_tokens),0)::bigint AS total_tokens, \
     COALESCE(SUM(c.total_cost_microunits),0)::bigint AS cost_microunits, \
     NULL::bigint AS avg_ttft_ms";

const USAGE_JOIN: &str = " FROM ai_requests r \
      LEFT JOIN usage_records u ON u.request_id = r.id \
      LEFT JOIN cost_records c ON c.request_id = r.id";

#[async_trait]
impl UsageRepository for PgUsageRepository {
    async fn summary(&self, query: &UsageQuery) -> Result<UsageAggregates> {
        let mut sql = format!("SELECT {AGG_SELECT}{USAGE_JOIN}");
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &query.application_id {
            sql.push_str(&format!(" WHERE r.application_id = ${}", binds.len() + 1));
            binds.push(app.clone());
        }
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let mut agg = agg_from_row(
            &q.fetch_one(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Request, e))?,
        );
        agg.p50_latency_ms = self.percentile(query, 0.5).await?;
        agg.p95_latency_ms = self.percentile(query, 0.95).await?;
        Ok(agg)
    }

    async fn timeseries(
        &self,
        query: &UsageQuery,
        bucket: &str,
    ) -> Result<Vec<(String, UsageAggregates)>> {
        let expr = match bucket {
            "hour" => "to_char(date_trunc('hour', r.started_at), 'YYYY-MM-DD\"T\"HH24:00:00\"Z\"')",
            _ => "to_char(date_trunc('day', r.started_at), 'YYYY-MM-DD')",
        };
        let mut sql = format!("SELECT {expr} AS bucket, {AGG_SELECT}{USAGE_JOIN}");
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &query.application_id {
            sql.push_str(&format!(" WHERE r.application_id = ${}", binds.len() + 1));
            binds.push(app.clone());
        }
        sql.push_str(" GROUP BY bucket ORDER BY bucket");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), agg_from_row(row)))
            .collect())
    }

    async fn by_model(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>> {
        let mut sql = format!("SELECT COALESCE(r.resolved_model_key, r.requested_model) AS bucket, {AGG_SELECT}{USAGE_JOIN}");
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &query.application_id {
            sql.push_str(&format!(" WHERE r.application_id = ${}", binds.len() + 1));
            binds.push(app.clone());
        }
        sql.push_str(" GROUP BY bucket ORDER BY requests DESC LIMIT 50");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), agg_from_row(row)))
            .collect())
    }

    async fn by_application(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>> {
        let mut sql = format!(
            "SELECT COALESCE(r.application_id, 'unattributed') AS bucket, {AGG_SELECT}{USAGE_JOIN}"
        );
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &query.application_id {
            sql.push_str(&format!(" WHERE r.application_id = ${}", binds.len() + 1));
            binds.push(app.clone());
        }
        sql.push_str(" GROUP BY bucket ORDER BY requests DESC LIMIT 50");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), agg_from_row(row)))
            .collect())
    }

    async fn by_user(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>> {
        let mut sql = format!(
            "SELECT COALESCE(r.user_id, 'anonymous') AS bucket, {AGG_SELECT}{USAGE_JOIN}"
        );
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &query.application_id {
            sql.push_str(&format!(" WHERE r.application_id = ${}", binds.len() + 1));
            binds.push(app.clone());
        }
        sql.push_str(" GROUP BY bucket ORDER BY requests DESC LIMIT 50");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), agg_from_row(row)))
            .collect())
    }

    async fn monthly_cost_for_application(
        &self,
        application_id: &str,
        month_start: DateTime<Utc>,
    ) -> Result<i64> {
        let row = sqlx::query("SELECT COALESCE(SUM(c.total_cost_microunits),0) AS v FROM ai_requests r JOIN cost_records c ON c.request_id = r.id WHERE r.application_id = $1 AND date_trunc('month', r.started_at) = date_trunc('month', $2::timestamptz)")
            .bind(application_id).bind(month_start)
            .fetch_one(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(row.get::<i64, _>("v"))
    }

    async fn monthly_tokens_for_application(
        &self,
        application_id: &str,
        month_start: DateTime<Utc>,
    ) -> Result<i64> {
        let row = sqlx::query("SELECT COALESCE(SUM(u.total_tokens),0) AS v FROM ai_requests r JOIN usage_records u ON u.request_id = r.id WHERE r.application_id = $1 AND date_trunc('month', r.started_at) = date_trunc('month', $2::timestamptz)")
            .bind(application_id).bind(month_start)
            .fetch_one(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(row.get::<i64, _>("v"))
    }
}

impl PgUsageRepository {
    async fn percentile(&self, query: &UsageQuery, q: f64) -> Result<Option<i64>> {
        let mut sql = String::from(
            "SELECT percentile_cont($1) WITHIN GROUP (ORDER BY r.latency_ms) AS v FROM ai_requests r WHERE r.latency_ms IS NOT NULL",
        );
        let mut binds: Vec<String> = Vec::new();
        if let Some(app) = &query.application_id {
            sql.push_str(&format!(" AND r.application_id = ${}", binds.len() + 2));
            binds.push(app.clone());
        }
        let mut query = sqlx::query(&sql).bind(q);
        for b in &binds {
            query = query.bind(b);
        }
        Ok(query.fetch_one(&self.pool).await.ok().and_then(|row| {
            row.try_get::<Option<f64>, _>("v")
                .ok()
                .and_then(|v| v.map(|x| x as i64))
        }))
    }
}

// ================= Audit =================

pub struct PgAuditRepository {
    pool: PgPool,
}

impl PgAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditRepository for PgAuditRepository {
    async fn insert(&self, event: AuditEvent) -> Result<()> {
        sqlx::query("INSERT INTO audit_events (id, trace_id, actor_type, actor_id, event_type, resource_type, resource_id, decision, payload_ref, metadata_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(&event.id).bind(&event.trace_id).bind(&event.actor_type)
            .bind(&event.actor_id).bind(&event.event_type)
            .bind(&event.resource_type).bind(&event.resource_id)
            .bind(&event.decision).bind(&event.payload_ref)
            .bind(&event.metadata).bind(event.created_at)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn list(&self, filter: &AuditFilter) -> Result<(Vec<AuditEvent>, u64)> {
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
            push(&format!("event_type = ${}", binds.len() + 1));
            binds.push(v.clone());
        }
        if let Some(v) = &filter.resource_type {
            push(&format!("resource_type = ${}", binds.len() + 1));
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
        let list_sql = format!("SELECT * FROM audit_events{where_clause} ORDER BY created_at DESC LIMIT {page_size} OFFSET {offset}");
        let mut list_query = sqlx::query(&list_sql);
        for b in &binds {
            list_query = list_query.bind(b);
        }
        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok((
            rows.iter()
                .map(|row| AuditEvent {
                    id: row.get("id"),
                    trace_id: row.get("trace_id"),
                    actor_type: row.get("actor_type"),
                    actor_id: row.get("actor_id"),
                    event_type: row.get("event_type"),
                    resource_type: row.get("resource_type"),
                    resource_id: row.get("resource_id"),
                    decision: row.get("decision"),
                    payload_ref: row.get("payload_ref"),
                    metadata: row.get::<Value, _>("metadata_json"),
                    created_at: row.get("created_at"),
                })
                .collect(),
            total as u64,
        ))
    }
}

// ================= Prompt =================

pub struct PgPromptRepository {
    pool: PgPool,
}

impl PgPromptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn prompt_from_row(row: &sqlx::postgres::PgRow) -> aihub_domain::prompt::Prompt {
    aihub_domain::prompt::Prompt {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        owner_id: row.get("owner_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn pv_from_row(row: &sqlx::postgres::PgRow) -> aihub_domain::prompt::PromptVersion {
    aihub_domain::prompt::PromptVersion {
        id: row.get("id"),
        prompt_id: row.get("prompt_id"),
        version: row.get("version"),
        status: aihub_domain::prompt::PromptVersionStatus::parse(&row.get::<String, _>("status"))
            .unwrap_or(aihub_domain::prompt::PromptVersionStatus::Draft),
        system_template: row.get("system_template"),
        user_template: row.get("user_template"),
        variables_schema: row.get::<Value, _>("variables_schema_json"),
        model_config: row.get::<Value, _>("model_config_json"),
        output_schema: row.get("output_schema_json"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
    }
}

#[async_trait]
impl aihub_domain::prompt::PromptRepository for PgPromptRepository {
    async fn create(
        &self,
        p: aihub_domain::prompt::NewPrompt,
    ) -> Result<aihub_domain::prompt::Prompt> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query("INSERT INTO prompts (id, key, name, description, owner_id, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(&id).bind(&p.key).bind(&p.name).bind(&p.description)
            .bind(&p.owner_id).bind(now).bind(now)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Prompt, e))?;
        sqlx::query("SELECT * FROM prompts WHERE id = $1")
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map(|r| prompt_from_row(&r))
            .map_err(|e| db_error(DomainResource::Prompt, e))
    }

    async fn get(&self, id: &str) -> Result<aihub_domain::prompt::Prompt> {
        sqlx::query("SELECT * FROM prompts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| prompt_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Prompt, id))
    }

    async fn get_by_key(&self, key: &str) -> Result<aihub_domain::prompt::Prompt> {
        sqlx::query("SELECT * FROM prompts WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| prompt_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Prompt, key))
    }

    async fn list(&self) -> Result<Vec<aihub_domain::prompt::Prompt>> {
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
    ) -> Result<aihub_domain::prompt::Prompt> {
        sqlx::query("UPDATE prompts SET name=$1, description=$2, updated_at=$3 WHERE id=$4")
            .bind(&name)
            .bind(&description)
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM prompts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        Ok(())
    }

    async fn create_version(
        &self,
        v: NewPromptVersion,
    ) -> Result<aihub_domain::prompt::PromptVersion> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(version), 0) AS v FROM prompt_versions WHERE prompt_id = $1",
        )
        .bind(&v.prompt_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Prompt, e))?;
        let next = row.get::<i32, _>("v") + 1;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO prompt_versions (id, prompt_id, version, status, system_template, user_template, variables_schema_json, model_config_json, output_schema_json, created_by, created_at) VALUES ($1,$2,$3,'draft',$4,$5,$6,$7,$8,$9,$10)")
            .bind(&id).bind(&v.prompt_id).bind(next)
            .bind(&v.system_template).bind(&v.user_template)
            .bind(&v.variables_schema).bind(&v.model_config).bind(&v.output_schema)
            .bind(&v.created_by).bind(Utc::now())
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get_version(&id).await
    }

    async fn versions_for(
        &self,
        prompt_id: &str,
    ) -> Result<Vec<aihub_domain::prompt::PromptVersion>> {
        let rows =
            sqlx::query("SELECT * FROM prompt_versions WHERE prompt_id = $1 ORDER BY version DESC")
                .bind(prompt_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Prompt, e))?;
        Ok(rows.iter().map(pv_from_row).collect())
    }

    async fn get_version(&self, version_id: &str) -> Result<aihub_domain::prompt::PromptVersion> {
        sqlx::query("SELECT * FROM prompt_versions WHERE id = $1")
            .bind(version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| pv_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Prompt, version_id))
    }

    async fn published_version(
        &self,
        prompt_id: &str,
    ) -> Result<Option<aihub_domain::prompt::PromptVersion>> {
        Ok(sqlx::query("SELECT * FROM prompt_versions WHERE prompt_id = $1 AND status = 'published' ORDER BY version DESC LIMIT 1")
            .bind(prompt_id).fetch_optional(&self.pool).await.map_err(|e| db_error(DomainResource::Prompt, e))?
            .map(|r| pv_from_row(&r)))
    }

    async fn set_version_status(
        &self,
        version_id: &str,
        status: aihub_domain::prompt::PromptVersionStatus,
    ) -> Result<aihub_domain::prompt::PromptVersion> {
        sqlx::query("UPDATE prompt_versions SET status = $1 WHERE id = $2")
            .bind(status.as_str())
            .bind(version_id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Prompt, e))?;
        self.get_version(version_id).await
    }
}

// ================= User =================

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn user_from_row(row: &sqlx::postgres::PgRow) -> User {
    User {
        id: row.get("id"),
        external_subject: row.get("external_subject"),
        identity_provider: row.get("identity_provider"),
        username: row.get("username"),
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        password_hash: row.get("password_hash"),
        created_at: row.get("created_at"),
    }
}

fn role_from_row(row: &sqlx::postgres::PgRow) -> Role {
    Role {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        system_role: row.get("system_role"),
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn create(&self, u: NewUser) -> Result<User> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query("INSERT INTO users (id, external_subject, identity_provider, username, email, display_name, status, password_hash, metadata_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,'active',$7,'{}',$8,$8)")
            .bind(&id).bind(&u.external_subject).bind(&u.identity_provider)
            .bind(&u.username).bind(&u.email).bind(&u.display_name)
            .bind(&u.password_hash).bind(now)
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::User, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<User> {
        sqlx::query("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?
            .map(|r| user_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::User, id))
    }

    async fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        Ok(sqlx::query("SELECT * FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?
            .map(|r| user_from_row(&r)))
    }

    async fn get_by_subject(&self, provider: &str, subject: &str) -> Result<Option<User>> {
        Ok(sqlx::query(
            "SELECT * FROM users WHERE identity_provider = $1 AND external_subject = $2",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::User, e))?
        .map(|r| user_from_row(&r)))
    }

    async fn list(&self) -> Result<Vec<User>> {
        let rows = sqlx::query("SELECT * FROM users ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(rows.iter().map(user_from_row).collect())
    }

    async fn set_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE users SET status=$1, updated_at=$2 WHERE id=$3")
            .bind(status)
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(())
    }

    async fn assign_role(&self, user_id: &str, role_key: &str) -> Result<()> {
        let role = sqlx::query("SELECT id FROM roles WHERE key = $1")
            .bind(role_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::User, role_key))?;
        sqlx::query("INSERT INTO user_roles (user_id, role_id, scope_type, created_at) VALUES ($1,$2,'global',$3) ON CONFLICT DO NOTHING")
            .bind(user_id).bind(role.get::<String, _>("id")).bind(Utc::now())
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::User, e))?;
        Ok(())
    }

    async fn roles_of(&self, user_id: &str) -> Result<Vec<Role>> {
        let rows = sqlx::query(
            "SELECT r.* FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1",
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
    ) -> Result<Role> {
        let existing = sqlx::query("SELECT id FROM roles WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        let role_id = match existing {
            Some(row) => row.get::<String, _>("id"),
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO roles (id, key, name, description, system_role, created_at) VALUES ($1,$2,$3,$4,TRUE,$5)")
                    .bind(&id).bind(key).bind(name).bind(description).bind(Utc::now())
                    .execute(&self.pool).await.map_err(|e| db_error(DomainResource::User, e))?;
                id
            }
        };
        for permission in permissions {
            let pid = match sqlx::query("SELECT id FROM permissions WHERE key = $1")
                .bind(permission)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::User, e))?
            {
                Some(row) => row.get::<String, _>("id"),
                None => {
                    let pid = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO permissions (id, key, description) VALUES ($1,$2,$2)")
                        .bind(&pid)
                        .bind(permission)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| db_error(DomainResource::User, e))?;
                    pid
                }
            };
            sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
                .bind(&role_id).bind(&pid)
                .execute(&self.pool).await.map_err(|e| db_error(DomainResource::User, e))?;
        }
        sqlx::query("SELECT * FROM roles WHERE id = $1")
            .bind(&role_id)
            .fetch_one(&self.pool)
            .await
            .map(|r| role_from_row(&r))
            .map_err(|e| db_error(DomainResource::User, e))
    }

    async fn list_roles(&self) -> Result<Vec<Role>> {
        let rows = sqlx::query("SELECT * FROM roles ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::User, e))?;
        Ok(rows.iter().map(role_from_row).collect())
    }
}

#[allow(unused_imports)]
use crate as _persistence_marker;

#[allow(unused)]
fn unused(_: DomainErrorCode) {}
