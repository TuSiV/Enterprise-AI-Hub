//! Admin API（方案 §13.1/§13.3）：统一 {data, meta} / {error} 信封 + Admin Token 鉴权。

use aihub_api_types::admin::*;
use aihub_api_types::common::{ApiErrorBody, PageMeta, PageResponse};
use aihub_domain::error::DomainError;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::AppState;

pub fn router(state: AppState) -> Router {
    let authed = Router::new()
        // system
        .route("/v1/admin/config", get(system_info))
        // providers
        .route("/v1/admin/providers", get(providers_list).post(providers_create))
        .route(
            "/v1/admin/providers/{id}",
            get(providers_get).patch(providers_update).delete(providers_delete),
        )
        .route("/v1/admin/providers/{id}/test", post(providers_test))
        .route("/v1/admin/providers/{id}/discover-models", post(providers_discover))
        // models
        .route("/v1/admin/models", get(models_list).post(models_create))
        .route(
            "/v1/admin/models/{id}",
            get(models_get).patch(models_update).delete(models_delete),
        )
        .route("/v1/admin/models/{id}/enable", post(models_enable))
        .route("/v1/admin/models/{id}/disable", post(models_disable))
        // virtual models
        .route(
            "/v1/admin/virtual-models",
            get(virtual_models_list).post(virtual_models_create),
        )
        .route(
            "/v1/admin/virtual-models/{id}",
            get(virtual_models_get).patch(virtual_models_update).delete(virtual_models_delete),
        )
        .route("/v1/admin/virtual-models/{id}/targets", put(virtual_models_targets))
        .route("/v1/admin/virtual-models/{id}/simulate-route", post(virtual_models_simulate))
        // applications / keys
        .route(
            "/v1/admin/applications",
            get(applications_list).post(applications_create),
        )
        .route(
            "/v1/admin/applications/{id}",
            get(applications_get).patch(applications_update).delete(applications_delete),
        )
        .route(
            "/v1/admin/applications/{id}/keys",
            get(keys_list).post(keys_create),
        )
        .route("/v1/admin/applications/{id}/keys/{keyId}", delete(keys_revoke))
        // usage / cost / requests / audit
        .route("/v1/admin/usage/summary", get(usage_summary))
        .route("/v1/admin/cost/summary", get(usage_summary))
        .route("/v1/admin/usage/timeseries", get(usage_timeseries))
        .route("/v1/admin/usage/by-model", get(usage_by_model))
        .route("/v1/admin/usage/by-application", get(usage_by_application))
        .route("/v1/admin/requests", get(requests_list))
        .route("/v1/admin/requests/{id}", get(requests_detail))
        .route("/v1/admin/audit", get(audit_list))
        // playground
        .route("/v1/admin/playground/run", post(playground_run))
        .route("/v1/admin/playground/stream", post(playground_stream))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), admin_auth));
    authed.with_state(state)
}

// ---------- 鉴权 ----------

async fn admin_auth(
    State(state): State<crate::AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let from_bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let from_header = req
        .headers()
        .get("x-aih-admin-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim);
    let provided = from_bearer.or(from_header);
    if provided != Some(state.admin_token.as_str()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiErrorBody::new("AIH_UNAUTHORIZED", "invalid or missing admin token")),
        )
            .into_response();
    }
    next.run(req).await
}

// ---------- 错误映射 ----------

fn domain_error_response(err: &DomainError) -> Response {
    let status = match err.code {
        aihub_domain::DomainErrorCode::NotFound => StatusCode::NOT_FOUND,
        aihub_domain::DomainErrorCode::Duplicate => StatusCode::CONFLICT,
        aihub_domain::DomainErrorCode::ValidationFailed => StatusCode::BAD_REQUEST,
        aihub_domain::DomainErrorCode::ResourceInUse => StatusCode::CONFLICT,
        aihub_domain::DomainErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(ApiErrorBody::new(err.code.as_str(), &err.message))).into_response()
}

fn pipeline_error_response(err: &aihub_application::error::PipelineError) -> Response {
    let status = StatusCode::from_u16(err.code.http_status()).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, Json(ApiErrorBody::new(err.code.as_str(), &err.message))).into_response()
}

// ---------- System ----------

async fn system_info(State(state): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        version: crate::VERSION.to_string(),
        mode: match state.config.mode {
            aihub_config::Mode::Desktop => "desktop".into(),
            aihub_config::Mode::Server => "server".into(),
        },
        gateway_endpoint: format!("http://{}:{}", state.config.gateway.host, state.config.gateway.port),
        db_driver: state.config.database.driver.clone(),
        started_at: state.started_at.to_rfc3339(),
    })
}

// ---------- Providers ----------

async fn providers_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.list().await {
        Ok(list) => Ok(Json(serde_json::json!({ "data": list, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_create(
    State(state): State<AppState>,
    Json(request): Json<CreateProviderRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.create(request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_get(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.get(&id).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProviderRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.update(&id, request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.delete(&id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "data": {"deleted": true}, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_test(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.test(&id).await {
        Ok(result) => Ok(Json(serde_json::json!({ "data": result, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_discover(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.discover_models(&id).await {
        Ok(result) => Ok(Json(serde_json::json!({ "data": result, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ---------- Models ----------

#[derive(Deserialize, Default)]
#[serde(default)]
struct ModelsQuery {
    #[serde(rename = "providerId")]
    provider_id: Option<String>,
    #[serde(rename = "type")]
    model_type: Option<String>,
    enabled: Option<bool>,
}

async fn models_list(
    State(state): State<AppState>,
    Query(query): Query<ModelsQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .models
        .list(query.provider_id, query.model_type, query.enabled)
        .await
    {
        Ok(list) => Ok(Json(serde_json::json!({ "data": list, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_create(
    State(state): State<AppState>,
    Json(request): Json<CreateModelRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.models.create(request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_get(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state.models.get(&id).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateModelRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.models.update(&id, request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state.models.delete(&id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "data": {"deleted": true}, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_enable(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state
        .models
        .update(
            &id,
            UpdateModelRequest {
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
    {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_disable(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    match state
        .models
        .update(
            &id,
            UpdateModelRequest {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
    {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ---------- Virtual Models ----------

async fn virtual_models_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.list().await {
        Ok(list) => Ok(Json(serde_json::json!({ "data": list, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn virtual_models_create(
    State(state): State<AppState>,
    Json(request): Json<CreateVirtualModelRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.create(request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn virtual_models_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.get(&id).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn virtual_models_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateVirtualModelRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.update(&id, request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn virtual_models_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.delete(&id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "data": {"deleted": true}, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn virtual_models_targets(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReplaceTargetsRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.replace_targets(&id, request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn virtual_models_simulate(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.virtual_models.simulate(&id).await {
        Ok(result) => Ok(Json(serde_json::json!({ "data": result, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ---------- Applications / Keys ----------

async fn applications_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.list().await {
        Ok(list) => Ok(Json(serde_json::json!({ "data": list, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn applications_create(
    State(state): State<AppState>,
    Json(request): Json<CreateApplicationRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.create(request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn applications_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.get(&id).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn applications_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateApplicationRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.update(&id, request).await {
        Ok(dto) => Ok(Json(serde_json::json!({ "data": dto, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn applications_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.delete(&id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "data": {"deleted": true}, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn keys_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.list_keys(&id).await {
        Ok(list) => Ok(Json(serde_json::json!({ "data": list, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn keys_create(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.create_key(&id, request).await {
        Ok(response) => Ok(Json(serde_json::json!({ "data": response, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn keys_revoke(
    State(state): State<AppState>,
    Path((id, key_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.applications.revoke_key(&id, &key_id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "data": {"revoked": true}, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ---------- Usage / Requests / Audit ----------

#[derive(Deserialize, Default)]
#[serde(default)]
struct UsageQueryParams {
    #[serde(rename = "applicationId")]
    application_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
    bucket: Option<String>,
    page: Option<u64>,
    #[serde(rename = "pageSize")]
    page_size: Option<u64>,
    status: Option<String>,
    model: Option<String>,
    #[serde(rename = "eventType")]
    event_type: Option<String>,
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
}

fn parse_time(value: &Option<String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, Response> {
    match value {
        Some(raw) => match chrono::DateTime::parse_from_rfc3339(raw) {
            Ok(t) => Ok(Some(t.with_timezone(&chrono::Utc))),
            Err(_) => Err((
                StatusCode::BAD_REQUEST,
                Json(ApiErrorBody::new("AIH_INVALID_REQUEST", "invalid RFC3339 timestamp")),
            )
                .into_response()),
        },
        None => Ok(None),
    }
}

async fn usage_summary(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let from = parse_time(&params.from)?;
    let to = parse_time(&params.to)?;
    let query = aihub_domain::repos::UsageQuery {
        application_id: params.application_id,
        from,
        to,
    };
    match state.repos.usage.summary(&query).await {
        Ok(agg) => {
            let dto = UsageSummary {
                requests: agg.requests,
                success_requests: agg.success_requests,
                failed_requests: agg.failed_requests,
                input_tokens: agg.input_tokens,
                output_tokens: agg.output_tokens,
                total_tokens: agg.total_tokens,
                cost_microunits: agg.cost_microunits,
                currency: "USD".into(),
                success_rate: if agg.requests > 0 {
                    agg.success_requests as f64 / agg.requests as f64
                } else {
                    0.0
                },
                p50_latency_ms: agg.p50_latency_ms,
                p95_latency_ms: agg.p95_latency_ms,
                avg_ttft_ms: agg.avg_ttft_ms,
                cache_hit_rate: if agg.input_tokens > 0 {
                    agg.cached_input_tokens as f64 / agg.input_tokens as f64
                } else {
                    0.0
                },
            };
            Ok(Json(serde_json::json!({ "data": dto, "meta": {} })))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn usage_timeseries(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let from = parse_time(&params.from)?;
    let to = parse_time(&params.to)?;
    let query = aihub_domain::repos::UsageQuery {
        application_id: params.application_id,
        from,
        to,
    };
    let bucket = params.bucket.unwrap_or_else(|| "day".into());
    match state.repos.usage.timeseries(&query, &bucket).await {
        Ok(rows) => {
            let data: Vec<TimeseriesPoint> = rows
                .into_iter()
                .map(|(bucket, agg)| TimeseriesPoint {
                    bucket,
                    requests: agg.requests,
                    input_tokens: agg.input_tokens,
                    output_tokens: agg.output_tokens,
                    cost_microunits: agg.cost_microunits,
                    errors: agg.failed_requests,
                })
                .collect();
            Ok(Json(serde_json::json!({ "data": data, "meta": {} })))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn usage_by_model(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let from = parse_time(&params.from)?;
    let to = parse_time(&params.to)?;
    let query = aihub_domain::repos::UsageQuery {
        application_id: params.application_id,
        from,
        to,
    };
    match state.repos.usage.by_model(&query).await {
        Ok(rows) => Ok(Json(serde_json::json!({
            "data": rows.into_iter().map(|(group, agg)| GroupUsage {
                group,
                requests: agg.requests,
                total_tokens: agg.total_tokens,
                cost_microunits: agg.cost_microunits,
            }).collect::<Vec<_>>(),
            "meta": {}
        }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn usage_by_application(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let from = parse_time(&params.from)?;
    let to = parse_time(&params.to)?;
    let query = aihub_domain::repos::UsageQuery {
        application_id: params.application_id,
        from,
        to,
    };
    match state.repos.usage.by_application(&query).await {
        Ok(rows) => {
            let mut data = Vec::new();
            for (group, agg) in rows {
                // 展示应用 key 而非 UUID
                let label = match state.repos.applications.get(&group).await {
                    Ok(app) => app.key,
                    Err(_) => group.clone(),
                };
                data.push(GroupUsage {
                    group: label,
                    requests: agg.requests,
                    total_tokens: agg.total_tokens,
                    cost_microunits: agg.cost_microunits,
                });
            }
            Ok(Json(serde_json::json!({ "data": data, "meta": {} })))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn requests_list(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(50).clamp(1, 200);
    match state
        .queries
        .list_requests(
            params.application_id,
            params.status,
            params.model,
            page,
            page_size,
        )
        .await
    {
        Ok(page_response) => {
            let PageResponse { data, meta } = page_response;
            let PageMeta { page, page_size, total } = meta;
            Ok(Json(serde_json::json!({ "data": data, "meta": {"page": page, "pageSize": page_size, "total": total} })))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn requests_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.queries.request_detail(&id).await {
        Ok(detail) => Ok(Json(serde_json::json!({ "data": detail, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn audit_list(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(50).clamp(1, 200);
    match state
        .queries
        .list_audit(params.event_type, params.resource_type, page, page_size)
        .await
    {
        Ok(page_response) => {
            let PageResponse { data, meta } = page_response;
            let PageMeta { page, page_size, total } = meta;
            Ok(Json(serde_json::json!({ "data": data, "meta": {"page": page, "pageSize": page_size, "total": total} })))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ---------- Playground ----------

async fn playground_run(
    State(state): State<AppState>,
    Json(request): Json<PlaygroundRunRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.playground.run(request).await {
        Ok(execution) => {
            let response = serde_json::json!({
                "requestId": execution.request_id,
                "traceId": execution.trace_id,
                "content": execution.response.content,
                "reasoningContent": execution.response.reasoning_content,
                "finishReason": execution.response.finish_reason,
                "resolvedModel": execution.resolved_model_key,
                "provider": execution.provider_key,
                "virtualModel": execution.virtual_model_key,
                "latencyMs": execution.latency_ms,
                "retryCount": execution.retry_count,
                "usage": execution.usage_cost.as_ref().map(|uc| serde_json::to_value(&uc.usage).unwrap_or_default()),
                "costMicrounits": execution.usage_cost.as_ref().map(|uc| uc.cost_microunits),
                "currency": execution.usage_cost.as_ref().map(|uc| uc.currency.clone()),
            });
            Ok(Json(serde_json::json!({ "data": response, "meta": {} })))
        }
        Err(e) => Err(pipeline_error_response(&e)),
    }
}

async fn playground_stream(
    State(state): State<AppState>,
    Json(request): Json<PlaygroundRunRequest>,
) -> Response {
    match state.playground.run_stream(request).await {
        Ok(execution) => {
            let mut rx = execution.rx;
            let event_stream = async_stream::stream! {
                while let Some(event) = rx.recv().await {
                    let payload = match event {
                        aihub_application::pipeline::PipelineStreamEvent::Content { delta } => {
                            serde_json::json!({"type": "content", "delta": delta})
                        }
                        aihub_application::pipeline::PipelineStreamEvent::Reasoning { delta } => {
                            serde_json::json!({"type": "reasoning", "delta": delta})
                        }
                        aihub_application::pipeline::PipelineStreamEvent::Started { resolved_model_key, provider_key, virtual_model_key } => {
                            serde_json::json!({"type": "started", "resolvedModel": resolved_model_key, "provider": provider_key, "virtualModel": virtual_model_key})
                        }
                        aihub_application::pipeline::PipelineStreamEvent::Completed { usage_cost, latency_ms, ttft_ms, resolved_model_key, provider_key, retry_count } => {
                            serde_json::json!({
                                "type": "completed",
                                "resolvedModel": resolved_model_key,
                                "provider": provider_key,
                                "latencyMs": latency_ms,
                                "ttftMs": ttft_ms,
                                "retryCount": retry_count,
                                "usage": usage_cost.as_ref().map(|uc| serde_json::to_value(&uc.usage).unwrap_or_default()),
                                "costMicrounits": usage_cost.as_ref().map(|uc| uc.cost_microunits),
                            })
                        }
                        aihub_application::pipeline::PipelineStreamEvent::Failed { code, message } => {
                            serde_json::json!({"type": "error", "code": code.as_str(), "message": message})
                        }
                        aihub_application::pipeline::PipelineStreamEvent::ToolCallStarted { index, id, name } => {
                            serde_json::json!({"type": "tool_call_started", "index": index, "id": id, "name": name})
                        }
                        aihub_application::pipeline::PipelineStreamEvent::ToolCallArguments { index, delta } => {
                            serde_json::json!({"type": "tool_call_args", "index": index, "delta": delta})
                        }
                    };
                    yield Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default().data(payload.to_string()),
                    );
                }
                yield Ok(axum::response::sse::Event::default().data("[DONE]"));
            };
            axum::response::sse::Sse::new(event_stream)
                .keep_alive(axum::response::sse::KeepAlive::default())
                .into_response()
        }
        Err(e) => pipeline_error_response(&e),
    }
}

#[allow(dead_code)]
async fn unused_headers(_: HeaderMap) {}
