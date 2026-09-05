//! GET /v1/models（§11.1）：返回允许的 Virtual Model 与（若允许）Physical Model。

use aihub_api_types::gateway as wire;
use aihub_application::error::{GatewayCode, PipelineError};
use aihub_application::pipeline::AuthContext;
use aihub_domain::repos::{ModelRepository, VirtualModelRepository};

use crate::GatewayState;

pub async fn list_models(
    state: &GatewayState,
    ctx: &AuthContext,
) -> Result<wire::ModelListResponse, PipelineError> {
    let mut data = Vec::new();

    let virtual_models = state
        .repos
        .virtual_models
        .list()
        .await
        .map_err(|e| PipelineError::from_domain(&e))?;
    for vm in virtual_models {
        if !vm.enabled {
            continue;
        }
        let allowed = ctx.application.allowed_virtual_models.is_empty()
            || ctx
                .application
                .allowed_virtual_models
                .iter()
                .any(|k| k == &vm.key);
        if allowed {
            data.push(wire::ModelInfo {
                id: vm.key.clone(),
                object: "model".into(),
                created: vm.created_at.timestamp(),
                owned_by: "aihub".into(),
                metadata: Some(serde_json::json!({"kind": "virtual"})),
            });
        }
    }

    if ctx.application.allow_direct_models {
        let models = state
            .repos
            .models
            .list(&aihub_domain::repos::ModelFilter {
                enabled: Some(true),
                ..Default::default()
            })
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        let providers = state
            .repos
            .providers
            .list(false)
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        for model in models {
            let provider = providers.iter().find(|p| p.id == model.provider_id);
            let Some(provider) = provider else { continue };
            if !provider.enabled {
                continue;
            }
            if data.iter().any(|m| m.id == model.model_key) {
                continue;
            }
            data.push(wire::ModelInfo {
                id: model.model_key.clone(),
                object: "model".into(),
                created: model.created_at.timestamp(),
                owned_by: provider.key.clone(),
                metadata: Some(serde_json::json!({"kind": "physical"})),
            });
        }
    }

    Ok(wire::ModelListResponse {
        object: "list".into(),
        data,
    })
}

#[allow(dead_code)]
fn unused(_: GatewayCode) {}
