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

/// 内置工具（§20.4 builtin）：echo / now / http_get（只读）。
pub async fn seed_builtin_tools(repos: &Repos) {
    use aihub_domain::platform::NewTool;
    let builtin: &[(&str, &str, &str, serde_json::Value, i64)] = &[
        (
            "echo",
            "Echo",
            "原样返回输入消息（连通性测试）",
            serde_json::json!({"type": "object", "properties": {"message": {"type": "string"}}, "required": ["message"]}),
            5000,
        ),
        (
            "now",
            "Now",
            "返回当前 UTC 时间",
            serde_json::json!({"type": "object", "properties": {}}),
            2000,
        ),
        (
            "http_get",
            "HTTP GET",
            "只读 HTTP GET 工具（URL 受 SSRF 策略约束）",
            serde_json::json!({"type": "object", "properties": {"url": {"type": "string", "description": "http(s) 地址"}}, "required": ["url"]}),
            15000,
        ),
    ];
    let kinds: &[&str] = &["builtin", "builtin", "http"];
    for ((key, name, description, schema, timeout), kind) in builtin.iter().zip(kinds) {
        if repos.tools.get_by_key(key).await.is_err() {
            if let Err(e) = repos
                .tools
                .create(NewTool {
                    key: key.to_string(),
                    name: name.to_string(),
                    description: Some(description.to_string()),
                    kind: kind.to_string(),
                    input_schema: schema.clone(),
                    config: serde_json::json!({}),
                    timeout_ms: *timeout,
                })
                .await
            {
                tracing::warn!(target: "aihub::seed", key, error = %e, "failed to seed builtin tool");
            }
        }
    }
}
