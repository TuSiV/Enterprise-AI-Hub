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

//! Admin API（方案 §13.1/§13.3）：统一 {data, meta} / {error} 信封 + Admin Token 鉴权。

use aihub_api_types::admin::*;
use aihub_api_types::common::{ApiErrorBody, PageMeta, PageResponse};
use aihub_domain::error::DomainError;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

pub fn router(state: AppState) -> Router {
    let authed = Router::new()
        // system
        .route("/v1/admin/config", get(system_info))
        // providers
        .route(
            "/v1/admin/providers",
            get(providers_list).post(providers_create),
        )
        .route(
            "/v1/admin/providers/{id}",
            get(providers_get)
                .patch(providers_update)
                .delete(providers_delete),
        )
        .route("/v1/admin/providers/{id}/test", post(providers_test))
        .route(
            "/v1/admin/providers/{id}/discover-models",
            post(providers_discover),
        )
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
            get(virtual_models_get)
                .patch(virtual_models_update)
                .delete(virtual_models_delete),
        )
        .route(
            "/v1/admin/virtual-models/{id}/targets",
            put(virtual_models_targets),
        )
        .route(
            "/v1/admin/virtual-models/{id}/simulate-route",
            post(virtual_models_simulate),
        )
        // applications / keys
        .route(
            "/v1/admin/applications",
            get(applications_list).post(applications_create),
        )
        .route(
            "/v1/admin/applications/{id}",
            get(applications_get)
                .patch(applications_update)
                .delete(applications_delete),
        )
        .route(
            "/v1/admin/applications/{id}/keys",
            get(keys_list).post(keys_create),
        )
        .route(
            "/v1/admin/applications/{id}/keys/{keyId}",
            delete(keys_revoke),
        )
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
        // M8 Prompt Center
        .route("/v1/admin/prompts", get(prompts_list).post(prompts_create))
        .route(
            "/v1/admin/prompts/{id}",
            get(prompts_detail)
                .patch(prompts_update)
                .delete(prompts_delete),
        )
        .route(
            "/v1/admin/prompts/{id}/versions",
            get(prompts_versions).post(prompts_create_version),
        )
        .route(
            "/v1/admin/prompt-versions/{versionId}/publish",
            post(prompts_publish),
        )
        .route(
            "/v1/admin/prompt-versions/{versionId}/deprecate",
            post(prompts_deprecate),
        )
        // M12/M13 Knowledge
        .route("/v1/admin/knowledge-bases", get(kb_list).post(kb_create))
        .route(
            "/v1/admin/knowledge-bases/{id}",
            get(kb_detail).delete(kb_delete),
        )
        .route(
            "/v1/admin/knowledge-bases/{id}/bind-embedding-model",
            post(kb_bind_model),
        )
        .route("/v1/admin/knowledge-bases/{id}/documents", get(doc_list))
        .route("/v1/admin/knowledge-bases/{id}/query", post(kb_query))
        .route(
            "/v1/admin/documents/{id}/upload-content",
            post(doc_upload_content),
        )
        .route(
            "/v1/admin/documents/{id}",
            get(doc_detail).delete(doc_delete),
        )
        // M14/M15 Agent / Tool / MCP
        .route("/v1/admin/tools", get(tools_list).post(tools_create))
        .route("/v1/admin/tools/{id}", delete(tools_delete))
        .route("/v1/admin/mcp-servers", get(mcp_list).post(mcp_create))
        .route("/v1/admin/mcp-servers/{id}", delete(mcp_delete))
        .route("/v1/admin/agents", get(agents_list).post(agents_create))
        .route(
            "/v1/admin/agents/{id}",
            get(agent_detail).delete(agents_delete),
        )
        .route(
            "/v1/admin/agents/{id}/versions",
            get(agent_versions).post(agent_create_version),
        )
        .route("/v1/admin/agents/{id}/run", post(agent_run))
        .route(
            "/v1/admin/agent-versions/{versionId}/publish",
            post(agent_publish),
        )
        .route(
            "/v1/admin/agent-versions/{versionId}/deprecate",
            post(agent_deprecate),
        )
        .route("/v1/admin/agent-runs/{runId}", get(agent_run_detail))
        // M16 Eval
        .route(
            "/v1/admin/evals/datasets",
            get(eval_datasets_list).post(eval_datasets_create),
        )
        .route(
            "/v1/admin/evals/datasets/{id}",
            get(eval_dataset_detail).delete(eval_datasets_delete),
        )
        .route(
            "/v1/admin/evals/datasets/{id}/cases",
            get(eval_cases_list).post(eval_cases_create),
        )
        .route(
            "/v1/admin/evals/datasets/{id}/runs",
            get(eval_runs_list).post(eval_run_create),
        )
        .route("/v1/admin/evals/runs/{runId}", get(eval_run_detail))
        // M17 Security / IAM
        .route(
            "/v1/admin/security/routing-policies",
            get(routing_policies_list).post(routing_policies_upsert),
        )
        .route(
            "/v1/admin/security/routing-policies/{id}",
            delete(routing_policies_delete),
        )
        .route(
            "/v1/admin/security/policies",
            get(security_policies_list).post(security_policies_upsert),
        )
        .route(
            "/v1/admin/security/policies/{id}",
            delete(security_policies_delete),
        )
        .route("/v1/admin/security/dlp/scan", post(dlp_scan))
        .route("/v1/admin/users", get(users_list).post(users_create))
        .route("/v1/auth/login", post(auth_login))
        .route("/v1/auth/oidc", post(auth_oidc))
        .route("/v1/admin/runtime/status", get(runtime_status))
        .route("/v1/admin/jobs", get(jobs_list))
        .route("/v1/admin/jobs/{id}/requeue", post(job_requeue))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            admin_auth,
        ));
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
    // Trusted upstream（§11.3）：反向代理注入的 X-AIH-User-ID，仅在配置显式开启时生效
    if state.config.auth.trusted_header_user {
        if let Some(user_id) = req
            .headers()
            .get("x-aih-user-id")
            .and_then(|v| v.to_str().ok())
        {
            if state
                .iam
                .identity_from_trusted_header(user_id, user_id)
                .await
                .is_ok()
            {
                return next.run(req).await;
            }
        }
    }
    let provided = from_bearer.or(from_header);
    // IAM session token（M10 RBAC）：aih_session_ 前缀走用户会话校验
    let session_ok = match provided {
        Some(token) if token.starts_with("aih_session_") => state
            .iam
            .session_user(token)
            .await
            .map(|u| u.status == "active")
            .unwrap_or(false),
        _ => false,
    };
    if provided != Some(state.admin_token.as_str()) && !session_ok {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiErrorBody::new(
                "AIH_UNAUTHORIZED",
                "invalid or missing admin token",
            )),
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
    (
        status,
        Json(ApiErrorBody::new(err.code.as_str(), &err.message)),
    )
        .into_response()
}

fn pipeline_error_response(err: &aihub_application::error::PipelineError) -> Response {
    let status = StatusCode::from_u16(err.code.http_status()).unwrap_or(StatusCode::BAD_GATEWAY);
    (
        status,
        Json(ApiErrorBody::new(err.code.as_str(), &err.message)),
    )
        .into_response()
}

// ---------- System ----------

async fn system_info(State(state): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        version: crate::VERSION.to_string(),
        mode: match state.config.mode {
            aihub_config::Mode::Desktop => "desktop".into(),
            aihub_config::Mode::Server => "server".into(),
        },
        gateway_endpoint: format!(
            "http://{}:{}",
            state.config.gateway.host, state.config.gateway.port
        ),
        db_driver: state.config.database.driver.clone(),
        started_at: state.started_at.to_rfc3339(),
    })
}

// ---------- Providers ----------

async fn providers_list(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Response> {
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

async fn providers_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
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

async fn providers_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.delete(&id).await {
        Ok(()) => Ok(Json(
            serde_json::json!({ "data": {"deleted": true}, "meta": {} }),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_test(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.providers.test(&id).await {
        Ok(result) => Ok(Json(serde_json::json!({ "data": result, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn providers_discover(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
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

async fn models_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
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

async fn models_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.models.delete(&id).await {
        Ok(()) => Ok(Json(
            serde_json::json!({ "data": {"deleted": true}, "meta": {} }),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn models_enable(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
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

async fn models_disable(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
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

async fn virtual_models_list(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Response> {
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
        Ok(()) => Ok(Json(
            serde_json::json!({ "data": {"deleted": true}, "meta": {} }),
        )),
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

async fn applications_list(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Response> {
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
        Ok(()) => Ok(Json(
            serde_json::json!({ "data": {"deleted": true}, "meta": {} }),
        )),
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
        Ok(()) => Ok(Json(
            serde_json::json!({ "data": {"revoked": true}, "meta": {} }),
        )),
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

#[allow(clippy::result_large_err)]
fn parse_time(value: &Option<String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, Response> {
    match value {
        Some(raw) => match chrono::DateTime::parse_from_rfc3339(raw) {
            Ok(t) => Ok(Some(t.with_timezone(&chrono::Utc))),
            Err(_) => Err((
                StatusCode::BAD_REQUEST,
                Json(ApiErrorBody::new(
                    "AIH_INVALID_REQUEST",
                    "invalid RFC3339 timestamp",
                )),
            )
                .into_response()),
        },
        None => Ok(None),
    }
}

#[allow(clippy::result_large_err)]
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
            let PageMeta {
                page,
                page_size,
                total,
            } = meta;
            Ok(Json(
                serde_json::json!({ "data": data, "meta": {"page": page, "pageSize": page_size, "total": total} }),
            ))
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
            let PageMeta {
                page,
                page_size,
                total,
            } = meta;
            Ok(Json(
                serde_json::json!({ "data": data, "meta": {"page": page, "pageSize": page_size, "total": total} }),
            ))
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

// ================= M8 Prompt Center =================

#[derive(Deserialize, Default)]
#[serde(default)]
#[allow(non_snake_case)]
struct PromptBody {
    key: Option<String>,
    name: Option<String>,
    description: Option<String>,
    systemTemplate: Option<String>,
    userTemplate: Option<String>,
    variablesSchema: Option<Value>,
    modelConfig: Option<Value>,
}

async fn prompts_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.prompts.list().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_create(
    State(state): State<AppState>,
    Json(body): Json<PromptBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .prompts
        .create(
            body.key.unwrap_or_default(),
            body.name.unwrap_or_default(),
            body.description,
        )
        .await
    {
        Ok(p) => Ok(Json(json!({"data": p, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.prompts.detail(&id).await {
        Ok(d) => Ok(Json(json!({"data": d, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PromptBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .prompts
        .update(&id, body.name.unwrap_or_default(), body.description)
        .await
    {
        Ok(p) => Ok(Json(json!({"data": p, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.prompts.delete(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_create_version(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PromptBody>,
) -> Result<Json<serde_json::Value>, Response> {
    let input = aihub_domain::prompt::NewPromptVersion {
        prompt_id: id.clone(),
        system_template: body.systemTemplate,
        user_template: body.userTemplate,
        variables_schema: body.variablesSchema.unwrap_or(json!({})),
        model_config: body.modelConfig.unwrap_or(json!({})),
        output_schema: None,
        created_by: None,
    };
    match state.prompts.create_version(&id, input).await {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_versions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.repos.prompts.versions_for(&id).await {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_publish(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.prompts.publish(&version_id).await {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn prompts_deprecate(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.prompts.deprecate(&version_id).await {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ================= M12/M13 Knowledge =================

#[derive(Deserialize, Default)]
#[serde(default)]
#[allow(non_snake_case)]
struct KbBody {
    key: Option<String>,
    name: Option<String>,
    visibility: Option<String>,
    embeddingModelId: Option<String>,
    query: Option<String>,
    topK: Option<usize>,
    filename: Option<String>,
    mimeType: Option<String>,
    content: Option<String>,
    modelId: Option<String>,
}

async fn kb_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.knowledge.list_kbs().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn kb_create(
    State(state): State<AppState>,
    Json(body): Json<KbBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .knowledge
        .create_kb(
            &body.key.unwrap_or_default(),
            &body.name.unwrap_or_default(),
            body.visibility.as_deref().unwrap_or("private"),
            body.embeddingModelId,
        )
        .await
    {
        Ok(kb) => Ok(Json(json!({"data": kb, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn kb_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.knowledge.list_kbs().await {
        Ok(kbs) => {
            let kb = kbs
                .into_iter()
                .find(|k| k.kb.id == id)
                .map(|k| k.kb)
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(ApiErrorBody::new("NOT_FOUND", "kb not found")),
                    )
                        .into_response()
                })?;
            let docs = state
                .knowledge
                .list_documents(&id)
                .await
                .unwrap_or_default();
            Ok(Json(
                json!({"data": {"kb": kb, "documents": docs}, "meta": {}}),
            ))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn kb_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.knowledge.delete_kb(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn kb_bind_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<KbBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .knowledge
        .bind_embedding_model(&id, &body.modelId.unwrap_or_default())
        .await
    {
        Ok(kb) => Ok(Json(json!({"data": kb, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn doc_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.knowledge.list_documents(&id).await {
        Ok(docs) => Ok(Json(json!({"data": docs, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

/// 文本内容上传（M12）：txt/md/csv/json 内联上传并同步索引
async fn doc_upload_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<KbBody>,
) -> Result<Json<serde_json::Value>, Response> {
    let content = body.content.unwrap_or_default();
    let filename = body.filename.unwrap_or_else(|| "document.txt".into());
    let mime = body.mimeType.unwrap_or_else(|| "text/plain".into());
    match state
        .knowledge
        .upload_document(&id, &filename, &mime, content.into_bytes(), None)
        .await
    {
        Ok(doc) => Ok(Json(json!({"data": doc, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn kb_query(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<KbBody>,
) -> Result<Json<serde_json::Value>, Response> {
    // id 兼容 key
    let key = match state.repos.knowledge.get_kb(&id).await {
        Ok(kb) => kb.key,
        Err(_) => id.clone(),
    };
    match state
        .knowledge
        .query(
            &key,
            &body.query.unwrap_or_default(),
            body.topK.unwrap_or(5),
        )
        .await
    {
        Ok(result) => Ok(Json(json!({"data": result, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn doc_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.knowledge.document_detail(&id).await {
        Ok((doc, chunks)) => {
            let chunk_preview: Vec<Value> = chunks
                .iter()
                .enumerate()
                .take(20)
                .map(|(i, c)| json!({"index": i, "tokens": c.token_count, "preview": c.content.chars().take(200).collect::<String>(), "embedded": c.embedding.is_some()}))
                .collect();
            Ok(Json(
                json!({"data": {"document": doc, "chunkCount": chunks.len(), "chunks": chunk_preview}, "meta": {}}),
            ))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn doc_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.knowledge.delete_document(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ================= M14/M15 Agent / Tool / MCP =================

#[derive(Deserialize, Default)]
#[serde(default)]
#[allow(non_snake_case)]
struct AgentBody {
    key: Option<String>,
    name: Option<String>,
    description: Option<String>,
    modelRef: Option<String>,
    systemPrompt: Option<String>,
    maxSteps: Option<i32>,
    maxToolCalls: Option<i32>,
    timeoutMs: Option<i64>,
    allowedTools: Option<Vec<String>>,
    input: Option<Value>,
    config: Option<Value>,
    kind: Option<String>,
    transport: Option<String>,
    endpointOrCommand: Option<String>,
    inputSchema: Option<Value>,
    timeoutSec: Option<i64>,
}

fn version_from_body(agent_id: &str, body: &AgentBody) -> aihub_domain::platform::NewAgentVersion {
    aihub_domain::platform::NewAgentVersion {
        agent_id: agent_id.to_string(),
        model_ref: body
            .modelRef
            .clone()
            .unwrap_or_else(|| "general-smart".into()),
        prompt_version_id: None,
        system_prompt: body.systemPrompt.clone(),
        max_steps: body.maxSteps.unwrap_or(8),
        max_tool_calls: body.maxToolCalls.unwrap_or(16),
        timeout_ms: body.timeoutMs.unwrap_or(120_000),
        max_cost_microunits: None,
        allowed_tools: body.allowedTools.clone().unwrap_or_default(),
        knowledge_bindings: vec![],
        status: "draft".into(),
    }
}

async fn tools_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.list_tools().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn tools_create(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .agents
        .create_tool(aihub_domain::platform::NewTool {
            key: body.key.unwrap_or_default(),
            name: body.name.unwrap_or_default(),
            description: body.description.clone(),
            kind: body.kind.clone().unwrap_or_else(|| "builtin".into()),
            input_schema: body.inputSchema.clone().unwrap_or(json!({})),
            config: body.config.clone().unwrap_or(json!({})),
            timeout_ms: (body.timeoutSec.unwrap_or(30)) * 1000,
        })
        .await
    {
        Ok(t) => Ok(Json(json!({"data": t, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn tools_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.delete_tool(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn mcp_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.list_mcp_servers().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn mcp_create(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .agents
        .create_mcp_server(aihub_domain::platform::NewMcpServer {
            key: body.key.unwrap_or_default(),
            name: body.name.unwrap_or_default(),
            transport: body
                .transport
                .clone()
                .unwrap_or_else(|| "streamable-http".into()),
            endpoint_or_command: body.endpointOrCommand.clone().unwrap_or_default(),
            config: body.config.clone().unwrap_or(json!({})),
        })
        .await
    {
        Ok(s) => Ok(Json(json!({"data": s, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn mcp_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.delete_mcp_server(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agents_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.list().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agents_create(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, Response> {
    let key = body.key.clone().unwrap_or_default();
    let name = body.name.clone().unwrap_or_default();
    match state
        .agents
        .create(
            &key,
            &name,
            body.description.clone(),
            version_from_body("", &body),
        )
        .await
    {
        Ok((agent, version)) => Ok(Json(
            json!({"data": {"agent": agent, "version": version}, "meta": {}}),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.repos.agents.get(&id).await {
        Ok(agent) => {
            let versions = state
                .repos
                .agents
                .versions_for(&id)
                .await
                .unwrap_or_default();
            Ok(Json(
                json!({"data": {"agent": agent, "versions": versions}, "meta": {}}),
            ))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agents_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.repos.agents.delete(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_versions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.versions(&id).await {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_create_version(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .agents
        .create_version(&id, version_from_body(&id, &body))
        .await
    {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_publish(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.publish(&version_id).await {
        Ok(v) => Ok(Json(json!({"data": v, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_deprecate(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.deprecate(&version_id).await {
        Ok(()) => Ok(Json(json!({"data": {"deprecated": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, Response> {
    // id 兼容 key
    let key = match state.repos.agents.get(&id).await {
        Ok(agent) => agent.key,
        Err(_) => id.clone(),
    };
    let ctx = aihub_application::pipeline::AuthContext {
        application: state
            .repos
            .applications
            .get_by_key(aihub_application::PLAYGROUND_APPLICATION_KEY)
            .await
            .map_err(|e| domain_error_response(&e))?,
        api_key_id: None,
        actor_type: "admin",
    };
    match state
        .agents
        .run_published(&key, body.input.unwrap_or(json!({})), &ctx)
        .await
    {
        Ok(result) => Ok(Json(json!({"data": result, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn agent_run_detail(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.agents.run_detail(&run_id).await {
        Ok((run, calls)) => Ok(Json(
            json!({"data": {"run": run, "toolCalls": calls}, "meta": {}}),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ================= M16 Eval =================

#[derive(Deserialize, Default)]
#[serde(default)]
#[allow(non_snake_case)]
struct EvalBody {
    key: Option<String>,
    name: Option<String>,
    description: Option<String>,
    label: Option<String>,
    candidate: Option<Value>,
    judgeConfig: Option<Value>,
    caseName: Option<String>,
    input: Option<Value>,
    expectedOutput: Option<String>,
}

async fn eval_datasets_list(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.evals.list_datasets().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_datasets_create(
    State(state): State<AppState>,
    Json(body): Json<EvalBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .evals
        .create_dataset(
            &body.key.unwrap_or_default(),
            &body.name.unwrap_or_default(),
            body.description,
        )
        .await
    {
        Ok(d) => Ok(Json(json!({"data": d, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_dataset_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let dataset = state
        .repos
        .evals
        .get_dataset(&id)
        .await
        .map_err(|e| domain_error_response(&e))?;
    let cases = state.repos.evals.list_cases(&id).await.unwrap_or_default();
    let runs = state.repos.evals.list_runs(&id).await.unwrap_or_default();
    Ok(Json(
        json!({"data": {"dataset": dataset, "cases": cases, "runs": runs}, "meta": {}}),
    ))
}

async fn eval_datasets_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.repos.evals.delete_dataset(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_cases_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.evals.list_cases(&id).await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_cases_create(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EvalBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .evals
        .add_case(
            &id,
            &body.caseName.clone().unwrap_or_else(|| "case".into()),
            body.input.clone().unwrap_or(json!({})),
            body.expectedOutput.clone(),
        )
        .await
    {
        Ok(c) => Ok(Json(json!({"data": c, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_runs_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.evals.runs(&id).await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_run_create(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EvalBody>,
) -> Result<Json<serde_json::Value>, Response> {
    let ctx = aihub_application::pipeline::AuthContext {
        application: state
            .repos
            .applications
            .get_by_key(aihub_application::PLAYGROUND_APPLICATION_KEY)
            .await
            .map_err(|e| domain_error_response(&e))?,
        api_key_id: None,
        actor_type: "admin",
    };
    match state
        .evals
        .run(
            &id,
            &body.label.clone().unwrap_or_else(|| "run".into()),
            body.candidate.clone().unwrap_or(json!({})),
            body.judgeConfig.clone().unwrap_or(json!({})),
            &ctx,
        )
        .await
    {
        Ok(outcome) => Ok(Json(json!({"data": outcome, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn eval_run_detail(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.evals.run_detail(&run_id).await {
        Ok((run, results)) => Ok(Json(
            json!({"data": {"run": run, "results": results}, "meta": {}}),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ================= M17 Security / IAM =================

#[derive(Deserialize, Default)]
#[serde(default)]
#[allow(non_snake_case)]
struct SecurityBody {
    key: Option<String>,
    name: Option<String>,
    policyType: Option<String>,
    priority: Option<i32>,
    rule: Option<Value>,
    action: Option<Value>,
    enabled: Option<bool>,
    content: Option<String>,
    classification: Option<String>,
    providerKind: Option<String>,
    id: Option<String>,
    username: Option<String>,
    password: Option<String>,
    displayName: Option<String>,
    role: Option<String>,
}

async fn routing_policies_list(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.policies.list_routing().await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn routing_policies_upsert(
    State(state): State<AppState>,
    Json(body): Json<SecurityBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .policies
        .upsert_routing_policy(aihub_domain::platform::RoutingPolicy {
            id: body.id.clone().unwrap_or_default(),
            key: body.key.clone().unwrap_or_default(),
            name: body.name.clone().unwrap_or_default(),
            priority: body.priority.unwrap_or(100),
            match_rules: body.rule.clone().unwrap_or(json!({})),
            action: body.action.clone().unwrap_or(json!({})),
            enabled: body.enabled.unwrap_or(true),
        })
        .await
    {
        Ok(()) => Ok(Json(json!({"data": {"saved": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn routing_policies_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.policies.delete_routing(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn security_policies_list(
    State(state): State<AppState>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<serde_json::Value>, Response> {
    let _ = &params;
    match state.policies.list_security(None).await {
        Ok(list) => Ok(Json(json!({"data": list, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn security_policies_upsert(
    State(state): State<AppState>,
    Json(body): Json<SecurityBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .policies
        .upsert_security_policy(aihub_domain::platform::SecurityPolicy {
            id: body.id.clone().unwrap_or_default(),
            key: body.key.clone().unwrap_or_default(),
            name: body.name.clone().unwrap_or_default(),
            policy_type: body.policyType.clone().unwrap_or_else(|| "dlp".into()),
            priority: body.priority.unwrap_or(100),
            rule: body.rule.clone().unwrap_or(json!({})),
            action: body.action.clone().unwrap_or(json!({})),
            enabled: body.enabled.unwrap_or(true),
        })
        .await
    {
        Ok(()) => Ok(Json(json!({"data": {"saved": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn security_policies_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.policies.delete_security(&id).await {
        Ok(()) => Ok(Json(json!({"data": {"deleted": true}, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn dlp_scan(
    State(state): State<AppState>,
    Json(body): Json<SecurityBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .policies
        .dlp_scan(&body.content.unwrap_or_default())
        .await
    {
        Ok(verdict) => Ok(Json(json!({"data": verdict, "meta": {}}))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn users_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.iam.list().await {
        Ok(list) => {
            let users: Vec<Value> = list
                .iter()
                .map(|u| json!({"id": u.id, "username": u.username, "displayName": u.display_name, "status": u.status, "identityProvider": u.identity_provider}))
                .collect();
            Ok(Json(json!({"data": users, "meta": {}})))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn users_create(
    State(state): State<AppState>,
    Json(body): Json<SecurityBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .iam
        .create_user(
            &body.username.clone().unwrap_or_default(),
            &body
                .displayName
                .clone()
                .unwrap_or_else(|| body.username.clone().unwrap_or_default()),
            body.password.as_deref(),
            body.role.as_deref().unwrap_or("developer"),
        )
        .await
    {
        Ok(user) => Ok(Json(
            json!({"data": {"id": user.id, "username": user.username, "displayName": user.display_name}, "meta": {}}),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn auth_login(
    State(state): State<AppState>,
    Json(body): Json<SecurityBody>,
) -> Result<Json<serde_json::Value>, Response> {
    match state
        .iam
        .login(
            &body.username.clone().unwrap_or_default(),
            &body.password.clone().unwrap_or_default(),
        )
        .await
    {
        Ok((user, token)) => Ok(Json(
            json!({"data": {"token": token, "user": {"id": user.id, "username": user.username}}, "meta": {}}),
        )),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ================= M11 Runtime 状态 =================

async fn runtime_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let runtime_state = state.runtime.state().await;
    let (status, endpoint, reason) = match runtime_state {
        aihub_runtime_client::RuntimeState::Disabled => ("disabled", None, None),
        aihub_runtime_client::RuntimeState::Starting => ("starting", None, None),
        aihub_runtime_client::RuntimeState::Running { endpoint } => {
            ("running", Some(endpoint), None)
        }
        aihub_runtime_client::RuntimeState::Failed { reason } => ("failed", None, Some(reason)),
    };
    Json(json!({
        "data": {"status": status, "endpoint": endpoint, "error": reason},
        "meta": {}
    }))
}

// ================= M11 Runtime Jobs =================

async fn jobs_list(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    match state.repos.jobs.list(50).await {
        Ok(list) => Ok(Json(json!({ "data": list, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

async fn job_requeue(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    match state.repos.jobs.requeue(&id).await {
        Ok(()) => Ok(Json(json!({ "data": {"requeued": true}, "meta": {} }))),
        Err(e) => Err(domain_error_response(&e)),
    }
}

// ================= OIDC 登录（§14.2） =================

#[derive(Deserialize, Default)]
#[serde(default)]
#[allow(non_snake_case)]
struct OidcLoginBody {
    idToken: Option<String>,
}

async fn auth_oidc(
    State(state): State<AppState>,
    Json(body): Json<OidcLoginBody>,
) -> Result<Json<serde_json::Value>, Response> {
    let (Some(jwks), Some(issuer), Some(audience)) = (
        state.config.auth.oidc_jwks_url.as_ref(),
        state.config.auth.oidc_issuer.as_ref(),
        state.config.auth.oidc_audience.as_ref(),
    ) else {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(ApiErrorBody::new(
                "AIH_FORBIDDEN",
                "OIDC login is not configured (set AIHUB_OIDC_* env)",
            )),
        )
            .into_response());
    };
    let id_token = body.idToken.unwrap_or_default();
    match state
        .iam
        .identity_from_oidc(jwks, &id_token, issuer, audience)
        .await
    {
        Ok(user) => {
            let token = state.iam.issue_session(&user).await;
            Ok(Json(
                json!({ "data": {"token": token, "user": {"id": user.id, "username": user.username, "displayName": user.display_name}}, "meta": {} }),
            ))
        }
        Err(e) => Err(domain_error_response(&e)),
    }
}
