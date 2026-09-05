//! POST /v1/embeddings（§11.1 扩展）：解析 embedding 模型并执行。
//! V1 不做多目标 failover，取第一候选执行。

use aihub_api_types::gateway as wire;
use aihub_application::error::{GatewayCode, PipelineError};
use aihub_application::pipeline::AuthContext;
use aihub_domain::canonical::{CanonicalEmbeddingRequest, CanonicalUsage};
use aihub_domain::entities::RequestStatus;
use aihub_domain::repos::{NewAiRequest, RequestFinish};
use serde_json::json;

use crate::GatewayState;

fn inputs_from(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(items) => items
            .iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        other => vec![other.to_string()],
    }
}

pub async fn execute_embeddings(
    state: &GatewayState,
    ctx: &AuthContext,
    request: wire::EmbeddingRequest,
) -> Result<wire::EmbeddingResponse, PipelineError> {
    let inputs = inputs_from(&request.input);
    if inputs.is_empty() {
        return Err(PipelineError::invalid_request("input must not be empty"));
    }

    let route = state
        .resolver
        .resolve(&request.model)
        .await
        .map_err(|e| PipelineError::from_domain(&e))?;
    if let Err(message) = aihub_application::resolver::ModelResolver::authorize(&ctx.application, &route) {
        return Err(PipelineError::new(GatewayCode::ModelNotAllowed, message));
    }
    let Some(candidate) = route.candidates.first().cloned() else {
        return Err(PipelineError::new(
            GatewayCode::RouteNotFound,
            "no available embedding target",
        ));
    };

    let request_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();
    state
        .repos
        .requests
        .create(NewAiRequest {
            id: request_id.clone(),
            trace_id: trace_id.clone(),
            application_id: Some(ctx.application.id.clone()),
            user_id: None,
            api_key_id: ctx.api_key_id.clone(),
            endpoint: "/v1/embeddings".to_string(),
            requested_model: request.model.clone(),
            metadata: json!({"inputs": inputs.len()}),
            started_at: chrono::Utc::now(),
        })
        .await
        .map_err(|e| PipelineError::from_domain(&e))?;

    let adapter = state
        .registry
        .get_for(&candidate.provider)
        .await
        .map_err(|e| PipelineError::from_domain(&e))?;
    let canonical_request = CanonicalEmbeddingRequest {
        model: candidate.model.model_key.clone(),
        inputs,
    };
    let started = std::time::Instant::now();
    let result = adapter.embeddings(canonical_request).await;
    match result {
        Ok(response) => {
            let usage = response.usage.clone().unwrap_or(CanonicalUsage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
                total_tokens: 0,
                source: aihub_domain::canonical::UsageSource::Estimated,
            });
            let _ = state.repos.requests.finish(
                &request_id,
                RequestFinish {
                    status: RequestStatus::Completed,
                    http_status: Some(200),
                    completed_at: chrono::Utc::now(),
                    ttft_ms: None,
                    latency_ms: Some(started.elapsed().as_millis() as i64),
                    retry_count: 0,
                    error_code: None,
                    error_message_safe: None,
                    resolved_model_id: Some(candidate.model.id.clone()),
                    resolved_model_key: Some(candidate.model.model_key.clone()),
                    provider_id: Some(candidate.provider.id.clone()),
                },
            );
            let _ = state.repos.requests.insert_usage(aihub_domain::entities::UsageRecord {
                id: uuid::Uuid::new_v4().to_string(),
                request_id: request_id.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                reasoning_tokens: usage.reasoning_tokens,
                total_tokens: usage.total_tokens,
                usage_source: "provider".into(),
                raw_usage: serde_json::to_value(&usage).unwrap_or_default(),
                created_at: chrono::Utc::now(),
            });
            let _ = state.repos.requests.insert_cost(aihub_domain::entities::CostRecord {
                id: uuid::Uuid::new_v4().to_string(),
                request_id: request_id.clone(),
                currency: candidate.model.pricing.currency.clone(),
                input_cost_microunits: 0,
                output_cost_microunits: 0,
                cache_cost_microunits: 0,
                reasoning_cost_microunits: 0,
                total_cost_microunits: aihub_domain::cost::calculate(&candidate.model.pricing, &usage)
                    .total_cost_microunits,
                pricing_snapshot: serde_json::to_value(&candidate.model.pricing).unwrap_or_default(),
                created_at: chrono::Utc::now(),
            });
            Ok(wire::EmbeddingResponse {
                object: "list".into(),
                data: response
                    .embeddings
                    .into_iter()
                    .enumerate()
                    .map(|(index, embedding)| wire::EmbeddingData {
                        object: "embedding".into(),
                        embedding: json!(embedding),
                        index,
                    })
                    .collect(),
                model: request.model.clone(),
                usage: Some(wire::Usage {
                    prompt_tokens: usage.input_tokens,
                    completion_tokens: usage.output_tokens,
                    total_tokens: usage.total_tokens,
                    prompt_tokens_details: None,
                    completion_tokens_details: None,
                }),
            })
        }
        Err(err) => {
            let pipeline_err = PipelineError::from_provider_error(&err);
            let _ = state.repos.requests.finish(
                &request_id,
                RequestFinish {
                    status: RequestStatus::Failed,
                    http_status: Some(pipeline_err.code.http_status() as i64),
                    completed_at: chrono::Utc::now(),
                    ttft_ms: None,
                    latency_ms: Some(started.elapsed().as_millis() as i64),
                    retry_count: 0,
                    error_code: Some(pipeline_err.code.as_str().to_string()),
                    error_message_safe: Some(pipeline_err.message.clone()),
                    resolved_model_id: Some(candidate.model.id.clone()),
                    resolved_model_key: Some(candidate.model.model_key.clone()),
                    provider_id: Some(candidate.provider.id.clone()),
                },
            );
            Err(pipeline_err)
        }
    }
}
