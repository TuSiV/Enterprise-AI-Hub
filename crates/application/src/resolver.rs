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

//! 模型解析（方案 §11.2）：VirtualModel key → Physical Model alias → direct model。

use aihub_domain::entities::{Application, Model, Provider, VirtualModel};
use aihub_domain::repos::{ModelRepository, ProviderRepository, VirtualModelRepository};
use aihub_domain::DomainResource;
use std::sync::Arc;

pub struct ModelResolver {
    virtual_models: Arc<dyn VirtualModelRepository>,
    models: Arc<dyn ModelRepository>,
    providers: Arc<dyn ProviderRepository>,
}

/// 解析结果：virtual model + 候选 target 列表，或 direct model 单候选。
pub struct ResolvedRoute {
    pub virtual_model: Option<VirtualModel>,
    /// 已按 priority 排序、且与 Application 授权校验无关的物理候选
    pub candidates: Vec<RouteCandidate>,
    pub direct: bool,
}

#[derive(Clone)]
pub struct RouteCandidate {
    pub model: Model,
    pub provider: Provider,
    pub priority: i32,
}

impl ModelResolver {
    pub fn new(
        virtual_models: Arc<dyn VirtualModelRepository>,
        models: Arc<dyn ModelRepository>,
        providers: Arc<dyn ProviderRepository>,
    ) -> Self {
        Self {
            virtual_models,
            models,
            providers,
        }
    }

    /// 解析顺序（§11.2）：VirtualModel 命中 → direct model → MODEL_NOT_FOUND。
    pub async fn resolve(
        &self,
        model_ref: &str,
    ) -> Result<ResolvedRoute, aihub_domain::DomainError> {
        // 1. Virtual Model
        if let Ok(virtual_model) = self.virtual_models.get_by_key(model_ref).await {
            if !virtual_model.enabled {
                return Err(aihub_domain::DomainError::validation(
                    DomainResource::VirtualModel,
                    format!("virtual model '{model_ref}' is disabled"),
                ));
            }
            let targets = self.virtual_models.targets_for(&virtual_model.id).await?;
            let mut candidates = Vec::new();
            for target in targets.iter().filter(|t| t.enabled) {
                if let Ok(model) = self.models.get(&target.model_id).await {
                    if !model.enabled {
                        continue;
                    }
                    if let Ok(provider) = self.providers.get(&model.provider_id).await {
                        candidates.push(RouteCandidate {
                            model,
                            provider,
                            priority: target.priority,
                        });
                    }
                }
            }
            candidates.sort_by_key(|c| c.priority);
            return Ok(ResolvedRoute {
                virtual_model: Some(virtual_model),
                candidates,
                direct: false,
            });
        }

        // 2. Direct physical model（按 model_key 精确匹配；重名冲突时拒绝）
        let models = self
            .models
            .list(&aihub_domain::repos::ModelFilter {
                enabled: Some(true),
                ..Default::default()
            })
            .await?;
        let matches: Vec<&Model> = models.iter().filter(|m| m.model_key == model_ref).collect();
        match matches.len() {
            0 => Err(aihub_domain::DomainError::not_found(
                DomainResource::Model,
                model_ref,
            )),
            1 => {
                let model = matches[0].clone();
                let provider = self.providers.get(&model.provider_id).await?;
                Ok(ResolvedRoute {
                    virtual_model: None,
                    candidates: vec![RouteCandidate {
                        model,
                        provider,
                        priority: 0,
                    }],
                    direct: true,
                })
            }
            _ => Err(aihub_domain::DomainError::validation(
                DomainResource::Model,
                format!(
                    "model key '{model_ref}' is ambiguous across providers; use a virtual model"
                ),
            )),
        }
    }

    /// Application 授权检查（§11.2/§14.3）。
    pub fn authorize(application: &Application, route: &ResolvedRoute) -> Result<(), String> {
        if route.direct && !application.allow_direct_models {
            return Err(format!(
                "application '{}' is not allowed to use direct physical models",
                application.key
            ));
        }
        if !application.allowed_virtual_models.is_empty() {
            let Some(vm) = &route.virtual_model else {
                return Err("direct models require empty allowed_virtual_models policy or explicit allowance".to_string());
            };
            if !application
                .allowed_virtual_models
                .iter()
                .any(|k| k == &vm.key)
            {
                return Err(format!(
                    "virtual model '{}' is not allowed for application '{}'",
                    vm.key, application.key
                ));
            }
        }
        Ok(())
    }
}
