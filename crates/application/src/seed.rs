//! 启动种子数据（§16.3 预置 Virtual Model 模板 + 本地 Playground 应用）。
//! 预置只是初始模板，管理员可随时重新映射（§16.3）。

use aihub_domain::entities::RoutingStrategy;
use aihub_domain::repos::{NewApplication, NewVirtualModel};
use serde_json::json;

use crate::Repos;

const DEFAULT_VIRTUAL_MODELS: &[(&str, &str, &str)] = &[
    ("general-fast", "General Fast", "低延迟日常任务"),
    ("general-smart", "General Smart", "通用智能任务默认入口"),
    ("reasoning", "Reasoning", "深度推理任务"),
    ("coding", "Coding", "编码任务"),
    ("embedding-default", "Embedding Default", "默认向量模型"),
];

pub const PLAYGROUND_APPLICATION: &str = crate::PLAYGROUND_APPLICATION_KEY;

pub async fn seed_defaults(repos: &Repos) {
    for (key, name, description) in DEFAULT_VIRTUAL_MODELS {
        if repos.virtual_models.get_by_key(key).await.is_err() {
            if let Err(e) = repos
                .virtual_models
                .create(
                    NewVirtualModel {
                        key: key.to_string(),
                        name: name.to_string(),
                        description: Some(description.to_string()),
                        routing_strategy: RoutingStrategy::PriorityFailover,
                        enabled: true,
                        config: json!({}),
                    },
                    vec![],
                )
                .await
            {
                tracing::warn!(target: "aihub::seed", key, error = %e, "failed to seed virtual model");
            } else {
                tracing::info!(target: "aihub::seed", key, "seeded virtual model");
            }
        }
    }

    if repos
        .applications
        .get_by_key(PLAYGROUND_APPLICATION)
        .await
        .is_err()
    {
        match repos
            .applications
            .create(NewApplication {
                key: PLAYGROUND_APPLICATION.to_string(),
                name: "Local Playground".to_string(),
                status: "active".to_string(),
                allowed_virtual_models: vec![],
                allow_direct_models: true,
                monthly_budget_microunits: None,
                metadata: json!({"system": true}),
            })
            .await
        {
            Ok(_) => {
                tracing::info!(target: "aihub::seed", key = PLAYGROUND_APPLICATION, "seeded playground application")
            }
            Err(e) => {
                tracing::warn!(target: "aihub::seed", error = %e, "failed to seed playground application")
            }
        }
    }
}
