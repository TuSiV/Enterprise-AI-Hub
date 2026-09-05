use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts, ts_to_string};

pub struct SqliteRequestRepository {
    pool: SqlitePool,
}

impl SqliteRequestRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn request_from_row(row: &sqlx::sqlite::SqliteRow) -> AiRequest {
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
        http_status: row.get("http_status"),
        started_at: parse_ts(Some(row.get("started_at"))).unwrap_or_default(),
        completed_at: parse_ts(row.get("completed_at")),
        ttft_ms: row.get("ttft_ms"),
        latency_ms: row.get("latency_ms"),
        retry_count: row.get::<i64, _>("retry_count") as i32,
        cache_status: row.get("cache_status"),
        error_code: row.get("error_code"),
        error_message_safe: row.get("error_message_safe"),
        metadata: parse_json(Some(row.get("metadata_json"))),
    }
}

#[async_trait]
impl RequestRepository for SqliteRequestRepository {
    async fn create(&self, request: NewAiRequest) -> Result<AiRequest, DomainError> {
        sqlx::query(
            "INSERT INTO ai_requests (id, trace_id, application_id, user_id, api_key_id, endpoint, requested_model, status, started_at, metadata_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'accepted', ?, ?)",
        )
        .bind(&request.id)
        .bind(&request.trace_id)
        .bind(&request.application_id)
        .bind(&request.user_id)
        .bind(&request.api_key_id)
        .bind(&request.endpoint)
        .bind(&request.requested_model)
        .bind(ts_to_string(request.started_at))
        .bind(json_string(&request.metadata))
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        self.get(&request.id).await
    }

    async fn finish(&self, id: &str, finish: RequestFinish) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE ai_requests SET status=?, http_status=?, completed_at=?, ttft_ms=?, latency_ms=?, retry_count=?, error_code=?, error_message_safe=?, resolved_model_id=?, resolved_model_key=?, provider_id=? WHERE id=?",
        )
        .bind(finish.status.as_str())
        .bind(finish.http_status)
        .bind(ts_to_string(finish.completed_at))
        .bind(finish.ttft_ms)
        .bind(finish.latency_ms)
        .bind(finish.retry_count as i64)
        .bind(&finish.error_code)
        .bind(&finish.error_message_safe)
        .bind(&finish.resolved_model_id)
        .bind(&finish.resolved_model_key)
        .bind(&finish.provider_id)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<AiRequest, DomainError> {
        let row = sqlx::query("SELECT * FROM ai_requests WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Request, id))?;
        Ok(request_from_row(&row))
    }

    async fn list(&self, filter: &RequestFilter) -> Result<(Vec<AiRequest>, u64), DomainError> {
        let mut where_clause = String::new();
        let mut binds: Vec<String> = Vec::new();
        let mut bind_ts: Vec<DateTime<Utc>> = Vec::new();
        let mut push = |clause: &str, where_clause: &mut String| {
            if where_clause.is_empty() {
                where_clause.push_str(" WHERE ");
            } else {
                where_clause.push_str(" AND ");
            }
            where_clause.push_str(clause);
        };
        if let Some(app) = &filter.application_id {
            push("application_id = ?", &mut where_clause);
            binds.push(app.clone());
        }
        if let Some(status) = &filter.status {
            push("status = ?", &mut where_clause);
            binds.push(status.clone());
        }
        if let Some(model) = &filter.model {
            push("(resolved_model_key = ? OR requested_model = ?)", &mut where_clause);
            binds.push(model.clone());
            binds.push(model.clone());
        }
        if let Some(from) = filter.from {
            push("started_at >= ?", &mut where_clause);
            bind_ts.push(from);
        }
        if let Some(to) = filter.to {
            push("started_at <= ?", &mut where_clause);
            bind_ts.push(to);
        }

        let count_sql = format!("SELECT COUNT(*) AS c FROM ai_requests{where_clause}");
        let mut count_query = sqlx::query(&count_sql);
        for b in &binds {
            count_query = count_query.bind(b);
        }
        for ts in &bind_ts {
            count_query = count_query.bind(ts_to_string(*ts));
        }
        let total = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?
            .get::<i64, _>("c");

        let page_size = filter.page_size.clamp(1, 200);
        let offset = filter.page.saturating_sub(1) * page_size;
        let list_sql = format!(
            "SELECT * FROM ai_requests{where_clause} ORDER BY started_at DESC LIMIT {page_size} OFFSET {offset}"
        );
        let mut list_query = sqlx::query(&list_sql);
        for b in &binds {
            list_query = list_query.bind(b);
        }
        for ts in &bind_ts {
            list_query = list_query.bind(ts_to_string(*ts));
        }
        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok((rows.iter().map(request_from_row).collect(), total as u64))
    }

    async fn insert_usage(&self, usage: UsageRecord) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO usage_records (id, request_id, input_tokens, output_tokens, cached_input_tokens, reasoning_tokens, total_tokens, usage_source, raw_usage_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&usage.id)
        .bind(&usage.request_id)
        .bind(usage.input_tokens)
        .bind(usage.output_tokens)
        .bind(usage.cached_input_tokens)
        .bind(usage.reasoning_tokens)
        .bind(usage.total_tokens)
        .bind(&usage.usage_source)
        .bind(json_string(&usage.raw_usage))
        .bind(ts_to_string(usage.created_at))
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn insert_cost(&self, cost: CostRecord) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO cost_records (id, request_id, currency, input_cost_microunits, output_cost_microunits, cache_cost_microunits, reasoning_cost_microunits, total_cost_microunits, pricing_snapshot_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&cost.id)
        .bind(&cost.request_id)
        .bind(&cost.currency)
        .bind(cost.input_cost_microunits)
        .bind(cost.output_cost_microunits)
        .bind(cost.cache_cost_microunits)
        .bind(cost.reasoning_cost_microunits)
        .bind(cost.total_cost_microunits)
        .bind(json_string(&cost.pricing_snapshot))
        .bind(ts_to_string(cost.created_at))
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }

    async fn usage_for_request(&self, request_id: &str) -> Result<Option<UsageRecord>, DomainError> {
        let row = sqlx::query("SELECT * FROM usage_records WHERE request_id = ?")
            .bind(request_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(row.map(|r| UsageRecord {
            id: r.get("id"),
            request_id: r.get("request_id"),
            input_tokens: r.get("input_tokens"),
            output_tokens: r.get("output_tokens"),
            cached_input_tokens: r.get("cached_input_tokens"),
            reasoning_tokens: r.get("reasoning_tokens"),
            total_tokens: r.get("total_tokens"),
            usage_source: r.get("usage_source"),
            raw_usage: parse_json(Some(r.get("raw_usage_json"))),
            created_at: parse_ts(Some(r.get("created_at"))).unwrap_or_default(),
        }))
    }

    async fn cost_for_request(&self, request_id: &str) -> Result<Option<CostRecord>, DomainError> {
        let row = sqlx::query("SELECT * FROM cost_records WHERE request_id = ?")
            .bind(request_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(row.map(|r| CostRecord {
            id: r.get("id"),
            request_id: r.get("request_id"),
            currency: r.get("currency"),
            input_cost_microunits: r.get("input_cost_microunits"),
            output_cost_microunits: r.get("output_cost_microunits"),
            cache_cost_microunits: r.get("cache_cost_microunits"),
            reasoning_cost_microunits: r.get("reasoning_cost_microunits"),
            total_cost_microunits: r.get("total_cost_microunits"),
            pricing_snapshot: parse_json(Some(r.get("pricing_snapshot_json"))),
            created_at: parse_ts(Some(r.get("created_at"))).unwrap_or_default(),
        }))
    }

    async fn update_resolved(&self, id: &str, model_id: &str, model_key: &str, provider_id: &str) -> Result<(), DomainError> {
        sqlx::query("UPDATE ai_requests SET resolved_model_id=?, resolved_model_key=?, provider_id=?, status='running' WHERE id=?")
            .bind(model_id)
            .bind(model_key)
            .bind(provider_id)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(())
    }
}

pub struct SqliteUsageRepository {
    pool: SqlitePool,
}

impl SqliteUsageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn aggregates_from_row(row: &sqlx::sqlite::SqliteRow) -> UsageAggregates {
    UsageAggregates {
        requests: row.get("requests"),
        success_requests: row.get("success_requests"),
        failed_requests: row.get("failed_requests"),
        input_tokens: row.get("input_tokens"),
        output_tokens: row.get("output_tokens"),
        cached_input_tokens: row.get("cached_input_tokens"),
        total_tokens: row.get("total_tokens"),
        cost_microunits: row.get("cost_microunits"),
        p50_latency_ms: row.get("p50_latency_ms"),
        p95_latency_ms: row.get("p95_latency_ms"),
        avg_ttft_ms: row.get("avg_ttft_ms"),
    }
}

fn aggregates_select_clause() -> &'static str {
    "COUNT(*) AS requests,
     COALESCE(SUM(CASE WHEN r.status = 'completed' THEN 1 ELSE 0 END),0) AS success_requests,
     COALESCE(SUM(CASE WHEN r.status IN ('failed','timeout') THEN 1 ELSE 0 END),0) AS failed_requests,
     COALESCE(SUM(u.input_tokens),0) AS input_tokens,
     COALESCE(SUM(u.output_tokens),0) AS output_tokens,
     COALESCE(SUM(u.cached_input_tokens),0) AS cached_input_tokens,
     COALESCE(SUM(u.total_tokens),0) AS total_tokens,
     COALESCE(SUM(c.total_cost_microunits),0) AS cost_microunits,
     NULL AS p50_latency_ms, NULL AS p95_latency_ms, NULL AS avg_ttft_ms"
}

fn usage_join() -> &'static str {
    " FROM ai_requests r
      LEFT JOIN usage_records u ON u.request_id = r.id
      LEFT JOIN cost_records c ON c.request_id = r.id"
}

fn apply_usage_query(mut sql: String, query: &UsageQuery, clause: &str, binds: &mut Vec<String>, ts_binds: &mut Vec<DateTime<Utc>>) -> String {
    sql.push_str(clause);
    if let Some(app) = &query.application_id {
        sql.push_str(" AND r.application_id = ?");
        binds.push(app.clone());
    }
    if let Some(from) = query.from {
        sql.push_str(" AND r.started_at >= ?");
        ts_binds.push(from);
    }
    if let Some(to) = query.to {
        sql.push_str(" AND r.started_at <= ?");
        ts_binds.push(to);
    }
    sql
}

async fn percentile_latency(
    pool: &SqlitePool,
    query: &UsageQuery,
    quantile: f64,
) -> Result<Option<i64>, DomainError> {
    fn build_sql(extra: &str, query: &UsageQuery) -> String {
        let mut sql = format!("SELECT {extra} FROM ai_requests r WHERE r.latency_ms IS NOT NULL");
        if query.application_id.is_some() {
            sql.push_str(" AND r.application_id = ?");
        }
        if query.from.is_some() {
            sql.push_str(" AND r.started_at >= ?");
        }
        if query.to.is_some() {
            sql.push_str(" AND r.started_at <= ?");
        }
        sql
    }
    fn bind_all<'q>(
        q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
        query: &'q UsageQuery,
    ) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
        let mut q = q;
        if let Some(app) = &query.application_id {
            q = q.bind(app);
        }
        if let Some(from) = query.from {
            q = q.bind(ts_to_string(from));
        }
        if let Some(to) = query.to {
            q = q.bind(ts_to_string(to));
        }
        q
    }

    let count = bind_all(sqlx::query(&build_sql("COUNT(*) AS c", query)), query)
        .fetch_one(pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?
        .get::<i64, _>("c");
    if count == 0 {
        return Ok(None);
    }
    let offset = ((count as f64) * quantile).floor() as i64;
    let offset = offset.clamp(0, count - 1);

    let value_sql = format!("{} ORDER BY r.latency_ms LIMIT 1 OFFSET {offset}", build_sql("r.latency_ms AS v", query));
    Ok(bind_all(sqlx::query(&value_sql), query)
        .fetch_one(pool)
        .await
        .ok()
        .and_then(|row| row.try_get::<Option<i64>, _>("v").ok().flatten()))
}

async fn avg_ttft(pool: &SqlitePool, query: &UsageQuery) -> Result<Option<i64>, DomainError> {
    let mut binds: Vec<String> = Vec::new();
    let mut ts_binds: Vec<DateTime<Utc>> = Vec::new();
    let mut sql = "SELECT AVG(r.ttft_ms) AS v FROM ai_requests r".to_string();
    sql = apply_usage_query(sql, query, " WHERE r.ttft_ms IS NOT NULL", &mut binds, &mut ts_binds);
    let mut q = sqlx::query(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    for ts in &ts_binds {
        q = q.bind(ts_to_string(*ts));
    }
    let row = q
        .fetch_one(pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
    Ok(row.get::<Option<f64>, _>("v").map(|v| v as i64))
}

async fn enrich_percentiles(pool: &SqlitePool, mut agg: UsageAggregates, query: &UsageQuery) -> Result<UsageAggregates, DomainError> {
    agg.p50_latency_ms = percentile_latency(pool, query, 0.5).await?;
    agg.p95_latency_ms = percentile_latency(pool, query, 0.95).await?;
    agg.avg_ttft_ms = avg_ttft(pool, query).await?;
    Ok(agg)
}

#[async_trait]
impl UsageRepository for SqliteUsageRepository {
    async fn summary(&self, query: &UsageQuery) -> Result<UsageAggregates, DomainError> {
        let mut binds: Vec<String> = Vec::new();
        let mut ts_binds: Vec<DateTime<Utc>> = Vec::new();
        let mut sql = format!("SELECT {}{}", aggregates_select_clause(), usage_join());
        sql = apply_usage_query(sql, query, " WHERE 1=1", &mut binds, &mut ts_binds);
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        for ts in &ts_binds {
            q = q.bind(ts_to_string(*ts));
        }
        let agg = aggregates_from_row(
            &q.fetch_one(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Request, e))?,
        );
        enrich_percentiles(&self.pool, agg, query).await
    }

    async fn timeseries(&self, query: &UsageQuery, bucket: &str) -> Result<Vec<(String, UsageAggregates)>, DomainError> {
        let expr = match bucket {
            "hour" => "substr(r.started_at,1,13) || ':00:00Z'",
            _ => "substr(r.started_at,1,10)",
        };
        let mut binds: Vec<String> = Vec::new();
        let mut ts_binds: Vec<DateTime<Utc>> = Vec::new();
        let mut sql = format!("SELECT {expr} AS bucket, {}{}", aggregates_select_clause(), usage_join());
        sql = apply_usage_query(sql, query, " WHERE 1=1", &mut binds, &mut ts_binds);
        sql.push_str(" GROUP BY bucket ORDER BY bucket");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        for ts in &ts_binds {
            q = q.bind(ts_to_string(*ts));
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), aggregates_from_row(row)))
            .collect())
    }

    async fn by_model(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>, DomainError> {
        let mut binds: Vec<String> = Vec::new();
        let mut ts_binds: Vec<DateTime<Utc>> = Vec::new();
        let mut sql = format!(
            "SELECT COALESCE(r.resolved_model_key, r.requested_model) AS bucket, {}{}",
            aggregates_select_clause(),
            usage_join()
        );
        sql = apply_usage_query(sql, query, " WHERE 1=1", &mut binds, &mut ts_binds);
        sql.push_str(" GROUP BY bucket ORDER BY requests DESC LIMIT 50");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        for ts in &ts_binds {
            q = q.bind(ts_to_string(*ts));
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), aggregates_from_row(row)))
            .collect())
    }

    async fn by_application(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>, DomainError> {
        let mut binds: Vec<String> = Vec::new();
        let mut ts_binds: Vec<DateTime<Utc>> = Vec::new();
        let mut sql = format!(
            "SELECT COALESCE(r.application_id, 'unattributed') AS bucket, {}{}",
            aggregates_select_clause(),
            usage_join()
        );
        sql = apply_usage_query(sql, query, " WHERE 1=1", &mut binds, &mut ts_binds);
        sql.push_str(" GROUP BY bucket ORDER BY requests DESC LIMIT 50");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        for ts in &ts_binds {
            q = q.bind(ts_to_string(*ts));
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("bucket"), aggregates_from_row(row)))
            .collect())
    }

    async fn monthly_cost_for_application(&self, application_id: &str, month_start: DateTime<Utc>) -> Result<i64, DomainError> {
        let next_month = month_start + Duration::days(31);
        let month_start_str = ts_to_string(month_start);
        // 用月起始时间字符串比较即可（RFC3339 UTC 定宽）
        let month_prefix = month_start_str[..7].to_string();
        let row = sqlx::query(
            "SELECT COALESCE(SUM(c.total_cost_microunits),0) AS v
             FROM ai_requests r JOIN cost_records c ON c.request_id = r.id
             WHERE r.application_id = ? AND substr(r.started_at,1,7) = ?",
        )
        .bind(application_id)
        .bind(month_prefix)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        let _ = next_month;
        Ok(row.get::<i64, _>("v"))
    }

    async fn monthly_tokens_for_application(&self, application_id: &str, month_start: DateTime<Utc>) -> Result<i64, DomainError> {
        let month_prefix = ts_to_string(month_start)[..7].to_string();
        let row = sqlx::query(
            "SELECT COALESCE(SUM(u.total_tokens),0) AS v
             FROM ai_requests r JOIN usage_records u ON u.request_id = r.id
             WHERE r.application_id = ? AND substr(r.started_at,1,7) = ?",
        )
        .bind(application_id)
        .bind(month_prefix)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Request, e))?;
        Ok(row.get::<i64, _>("v"))
    }
}

#[allow(dead_code)]
fn unused_now_helper() -> String {
    now_rfc3339()
}
