//! Admin 用例服务（方案 §13.1）：Provider/Model/VirtualModel/Application 管理与审计。

use std::sync::Arc;

use aihub_api_types::admin::*;
use aihub_api_types::common::{PageMeta, PageResponse};
use aihub_domain::cost::Pricing;
use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use aihub_secrets::{SecretRef, SecretStore};
use serde_json::json;

use crate::apikey;
use crate::registry::ProviderRegistry;
use crate::resolver::ModelResolver;
use crate::Repos;

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

async fn audit(repos: &Repos, event_type: &str, resource: &str, resource_id: &str, metadata: serde_json::Value) {
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        trace_id: None,
        actor_type: "admin".to_string(),
        actor_id: None,
        event_type: event_type.to_string(),
        resource_type: Some(resource.to_string()),
        resource_id: Some(resource_id.to_string()),
        decision: None,
        payload_ref: None,
        metadata,
        created_at: now(),
    };
    let _ = repos.audit.insert(event).await;
}

// ================= Provider =================

pub struct ProviderService {
    repos: Repos,
    registry: Arc<ProviderRegistry>,
    secrets: Arc<dyn SecretStore>,
}

impl ProviderService {
    pub fn new(repos: Repos, registry: Arc<ProviderRegistry>, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            repos,
            registry,
            secrets,
        }
    }

    async fn dto(&self, provider: &Provider, model_count: i64) -> ProviderDto {
        // credential_configured 来自数据库标记；读取 SecretStore 本身会触发 macOS 授权弹窗
        let credential_configured = provider.credential_configured;
        ProviderDto {
            id: provider.id.clone(),
            key: provider.key.clone(),
            name: provider.name.clone(),
            kind: provider.kind.as_str().to_string(),
            base_url: provider.base_url.clone(),
            credential_ref: provider.credential_ref.clone(),
            credential_configured,
            timeout_ms: provider.timeout_ms,
            max_retries: provider.max_retries,
            enabled: provider.enabled,
            status: provider.status.clone(),
            health: provider.health.clone(),
            last_health_check_at: provider.last_health_check_at.map(|t| t.to_rfc3339()),
            model_count,
            config: provider.config.clone(),
            created_at: provider.created_at.to_rfc3339(),
            updated_at: provider.updated_at.to_rfc3339(),
        }
    }

    pub async fn create(&self, request: CreateProviderRequest) -> Result<ProviderDto, DomainError> {
        if request.key.trim().is_empty() {
            return Err(DomainError::validation(DomainResource::Provider, "key is required"));
        }
        let Some(kind) = ProviderKind::parse(&request.kind) else {
            return Err(DomainError::validation(
                DomainResource::Provider,
                format!("unknown provider kind '{}'", request.kind),
            ));
        };
        if !self
            .registry
            .supported_protocols()
            .contains(&match kind {
                ProviderKind::OpenAI | ProviderKind::OpenAICompatible | ProviderKind::Ollama => "openai_compatible",
                ProviderKind::Anthropic => "anthropic",
                ProviderKind::Gemini => "gemini",
            })
        {
            return Err(DomainError::validation(
                DomainResource::Provider,
                format!("provider kind '{}' has no adapter registered yet", request.kind),
            ));
        }
        let provider = self
            .repos
            .providers
            .create(NewProvider {
                key: request.key.clone(),
                name: request.name.clone(),
                kind,
                base_url: request.base_url.clone(),
                proxy_url: None,
                timeout_ms: request.timeout_ms.unwrap_or(120_000),
                max_retries: request.max_retries.unwrap_or(2),
                enabled: request.enabled,
                status: if request.enabled { "active".into() } else { "disabled".into() },
                config: request.config.clone(),
            })
            .await?;

        if let Some(api_key) = &request.api_key {
            if !api_key.is_empty() {
                self.secrets
                    .set(&SecretRef(provider.credential_ref.clone().unwrap_or_default()), aihub_secrets::SecretValue::new(api_key))
                    .await
                    .map_err(|e| DomainError::internal(DomainResource::Provider, format!("secret store write failed: {e}")))?;
                self.repos
                    .providers
                    .update(
                        &provider.id,
                        ProviderUpdate {
                            credential_configured: Some(true),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
        }
        self.registry.invalidate(&provider.id).await;
        audit(&self.repos, "provider.created", "provider", &provider.id, json!({"key": provider.key})).await;
        let count = self.repos.models.count_by_provider(&provider.id).await?;
        Ok(self.dto(&provider, count).await)
    }

    pub async fn update(&self, id: &str, request: UpdateProviderRequest) -> Result<ProviderDto, DomainError> {
        let existing = self.repos.providers.get(id).await?;
        let update = ProviderUpdate {
            name: request.name,
            base_url: request.base_url,
            credential_configured: None,
            proxy_url: None,
            timeout_ms: request.timeout_ms,
            max_retries: request.max_retries,
            enabled: request.enabled,
            status: request.enabled.map(|enabled| if enabled { "active" } else { "disabled" }.to_string()),
            config: request.config,
            credential_ref: None,
        };
        let credential_rotated = request.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);
        let provider = self.repos.providers.update(id, update).await?;
        if let Some(api_key) = &request.api_key {
            if !api_key.is_empty() {
                self.secrets
                    .set(
                        &SecretRef(provider.credential_ref.clone().unwrap_or_default()),
                        aihub_secrets::SecretValue::new(api_key),
                    )
                    .await
                    .map_err(|e| DomainError::internal(DomainResource::Provider, format!("secret store write failed: {e}")))?;
                self.repos
                    .providers
                    .update(
                        id,
                        ProviderUpdate {
                            credential_configured: Some(true),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
        }
        if credential_rotated {
            self.registry.invalidate(id).await;
        }
        self.registry.invalidate(id).await;
        audit(&self.repos, "provider.updated", "provider", id, json!({"key": existing.key})).await;
        let count = self.repos.models.count_by_provider(id).await?;
        Ok(self.dto(&provider, count).await)
    }

    pub async fn delete(&self, id: &str) -> Result<(), DomainError> {
        let provider = self.repos.providers.get(id).await?;
        let count = self.repos.models.count_by_provider(id).await?;
        if count > 0 {
            return Err(DomainError::in_use(
                DomainResource::Provider,
                id,
                format!("provider has {count} models; remove them first"),
            ));
        }
        self.repos.providers.delete(id).await?;
        if let Some(reference) = &provider.credential_ref {
            let _ = self.secrets.delete(&SecretRef(reference.clone())).await;
        }
        self.registry.invalidate(id).await;
        audit(&self.repos, "provider.deleted", "provider", id, json!({"key": provider.key})).await;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<ProviderDto, DomainError> {
        let provider = self.repos.providers.get(id).await?;
        let count = self.repos.models.count_by_provider(id).await?;
        Ok(self.dto(&provider, count).await)
    }

    pub async fn list(&self) -> Result<Vec<ProviderDto>, DomainError> {
        let providers = self.repos.providers.list(false).await?;
        let mut dtos = Vec::new();
        for provider in &providers {
            let count = self.repos.models.count_by_provider(&provider.id).await?;
            dtos.push(self.dto(provider, count).await);
        }
        Ok(dtos)
    }

    /// 连接测试（§22.3）：调用 Provider 健康检查并记录样本。
    pub async fn test(&self, id: &str) -> Result<TestConnectionResult, DomainError> {
        let provider = self.repos.providers.get(id).await?;
        let adapter = self.registry.get_for(&provider).await?;
        let started = std::time::Instant::now();
        let health = adapter.health_check().await;
        let latency = started.elapsed().as_millis() as i64;

        let (ok, status, error, category) = match &health {
            Ok(h) => (
                h.status == "healthy",
                h.http_status,
                h.message.clone(),
                h.error_category.map(|c| c.as_str().to_string()),
            ),
            Err(e) => (false, e.http_status, Some(e.message.clone()), Some(e.category.as_str().to_string())),
        };
        let _ = self.repos.provider_health.insert_sample(NewHealthSample {
            provider_id: id.to_string(),
            model_id: None,
            status: if ok { "healthy".into() } else { "unavailable".into() },
            latency_ms: Some(latency),
            http_status: status.map(|s| s as i64),
            error_category: category,
        });
        let _ = self
            .repos
            .providers
            .set_health(id, if ok { "healthy" } else { "unavailable" }, now())
            .await;
        audit(&self.repos, "provider.tested", "provider", id, json!({"ok": ok})).await;

        Ok(TestConnectionResult {
            ok,
            latency_ms: Some(latency),
            error,
            checked_at: now().to_rfc3339(),
        })
    }

    /// 模型发现（§22.3）：拉取远端模型列表并 upsert。
    pub async fn discover_models(&self, id: &str) -> Result<DiscoverModelsResult, DomainError> {
        let provider = self.repos.providers.get(id).await?;
        let adapter = self.registry.get_for(&provider).await?;
        let discovered = adapter.list_models().await.map_err(|e| {
            DomainError::internal(DomainResource::Provider, format!("model discovery failed: {}", e.message))
        })?;
        let mut created = 0usize;
        let mut updated = 0usize;
        let mut dtos = Vec::new();
        for model in discovered {
            let model_type = model
                .model_type
                .as_deref()
                .and_then(ModelType::parse)
                .unwrap_or(ModelType::Chat);
            let existing = self
                .repos
                .models
                .list(&ModelFilter {
                    provider_id: Some(id.to_string()),
                    ..Default::default()
                })
                .await?
                .iter()
                .any(|m| m.model_key == model.model_key);
            let saved = self
                .repos
                .models
                .upsert_discovered(NewModel {
                    provider_id: id.to_string(),
                    model_key: model.model_key.clone(),
                    display_name: model.display_name.unwrap_or_else(|| model.model_key.clone()),
                    model_type,
                    context_window: model.context_window,
                    max_output_tokens: None,
                    capabilities: model.capabilities.clone().unwrap_or(json!({})),
                    pricing: Pricing::default(),
                    enabled: true,
                    discovered: true,
                    metadata: json!({}),
                })
                .await;
            match saved {
                Ok(m) => {
                    if existing {
                        updated += 1;
                    } else {
                        created += 1;
                    }
                    dtos.push(model_dto(&m, Some(&provider), None));
                }
                Err(_) => { /* 已存在且 upsert 冲突：跳过 */ }
            }
        }
        audit(&self.repos, "provider.models_discovered", "provider", id, json!({"created": created, "updated": updated})).await;
        Ok(DiscoverModelsResult {
            discovered: dtos.len(),
            created,
            updated,
            models: dtos,
        })
    }
}

// ================= Model =================

pub fn model_dto(model: &Model, provider: Option<&Provider>, provider_key: Option<&str>) -> ModelDto {
    ModelDto {
        id: model.id.clone(),
        provider_id: model.provider_id.clone(),
        provider_key: provider.map(|p| p.key.clone()).or_else(|| provider_key.map(|s| s.to_string())),
        provider_name: provider.map(|p| p.name.clone()),
        model_key: model.model_key.clone(),
        display_name: model.display_name.clone(),
        model_type: model.model_type.as_str().to_string(),
        context_window: model.context_window,
        max_output_tokens: model.max_output_tokens,
        capabilities: model.capabilities.clone(),
        pricing: serde_json::to_value(&model.pricing).unwrap_or_default(),
        enabled: model.enabled,
        discovered: model.discovered,
        metadata: model.metadata.clone(),
        created_at: model.created_at.to_rfc3339(),
        updated_at: model.updated_at.to_rfc3339(),
    }
}

pub struct ModelService {
    repos: Repos,
}

impl ModelService {
    pub fn new(repos: Repos) -> Self {
        Self { repos }
    }

    pub async fn create(&self, request: CreateModelRequest) -> Result<ModelDto, DomainError> {
        let provider = self.repos.providers.get(&request.provider_id).await?;
        let Some(model_type) = ModelType::parse(&request.model_type) else {
            return Err(DomainError::validation(DomainResource::Model, format!("unknown model type '{}'", request.model_type)));
        };
        let model = self
            .repos
            .models
            .create(NewModel {
                provider_id: request.provider_id.clone(),
                model_key: request.model_key.clone(),
                display_name: request.display_name.clone(),
                model_type,
                context_window: request.context_window,
                max_output_tokens: request.max_output_tokens,
                capabilities: request.capabilities.clone().unwrap_or(json!({})),
                pricing: request
                    .pricing
                    .as_ref()
                    .map(|p| serde_json::from_value(p.clone()).unwrap_or_default())
                    .unwrap_or_default(),
                enabled: request.enabled,
                discovered: false,
                metadata: json!({}),
            })
            .await?;
        audit(&self.repos, "model.created", "model", &model.id, json!({"key": model.model_key})).await;
        Ok(model_dto(&model, Some(&provider), None))
    }

    pub async fn update(&self, id: &str, request: UpdateModelRequest) -> Result<ModelDto, DomainError> {
        let model_type = request.model_type.as_deref().and_then(ModelType::parse);
        let pricing = request
            .pricing
            .as_ref()
            .map(|p| serde_json::from_value::<Pricing>(p.clone()))
            .transpose()
            .map_err(|e| DomainError::validation(DomainResource::Model, format!("invalid pricing: {e}")))?;
        let model = self
            .repos
            .models
            .update(
                id,
                ModelUpdate {
                    display_name: request.display_name,
                    model_type,
                    context_window: request.context_window.map(|v| Some(v)),
                    max_output_tokens: request.max_output_tokens.map(|v| Some(v)),
                    capabilities: request.capabilities,
                    pricing,
                    enabled: request.enabled,
                    metadata: request.metadata,
                },
            )
            .await?;
        audit(&self.repos, "model.updated", "model", id, json!({})).await;
        let provider = self.repos.providers.get(&model.provider_id).await?;
        Ok(model_dto(&model, Some(&provider), None))
    }

    pub async fn delete(&self, id: &str) -> Result<(), DomainError> {
        self.repos.models.delete(id).await?;
        audit(&self.repos, "model.deleted", "model", id, json!({})).await;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<ModelDto, DomainError> {
        let model = self.repos.models.get(id).await?;
        let provider = self.repos.providers.get(&model.provider_id).await?;
        Ok(model_dto(&model, Some(&provider), None))
    }

    pub async fn list(
        &self,
        provider_id: Option<String>,
        model_type: Option<String>,
        enabled: Option<bool>,
    ) -> Result<Vec<ModelDto>, DomainError> {
        let models = self
            .repos
            .models
            .list(&ModelFilter {
                provider_id,
                model_type,
                enabled,
            })
            .await?;
        let providers = self.repos.providers.list(false).await?;
        Ok(models
            .iter()
            .map(|m| {
                let provider = providers.iter().find(|p| p.id == m.provider_id);
                model_dto(m, provider, None)
            })
            .collect())
    }
}

// ================= Virtual Model =================

pub struct VirtualModelService {
    repos: Repos,
}

impl VirtualModelService {
    pub fn new(repos: Repos) -> Self {
        Self { repos }
    }

    /// 路径参数兼容 UUID 与 key（OpenAI 客户端友好）。
    async fn resolve_id(&self, id_or_key: &str) -> Result<String, DomainError> {
        if self.repos.virtual_models.get(id_or_key).await.is_ok() {
            return Ok(id_or_key.to_string());
        }
        Ok(self.repos.virtual_models.get_by_key(id_or_key).await?.id)
    }

    async fn dto(&self, vm: &VirtualModel) -> Result<VirtualModelDto, DomainError> {
        let targets = self.repos.virtual_models.targets_for(&vm.id).await?;
        let mut target_dtos = Vec::new();
        for target in &targets {
            let label = match self.repos.models.get(&target.model_id).await {
                Ok(m) => Some(format!("{} ({})", m.display_name, m.model_key)),
                Err(_) => None,
            };
            target_dtos.push(VirtualModelTargetDto {
                id: target.id.clone(),
                model_id: target.model_id.clone(),
                model_label: label,
                priority: target.priority,
                weight: target.weight,
                enabled: target.enabled,
                condition: target.condition.clone(),
                overrides: target.overrides.clone(),
            });
        }
        Ok(VirtualModelDto {
            id: vm.id.clone(),
            key: vm.key.clone(),
            name: vm.name.clone(),
            description: vm.description.clone(),
            routing_strategy: vm.routing_strategy.as_str().to_string(),
            enabled: vm.enabled,
            config: vm.config.clone(),
            targets: target_dtos,
            created_at: vm.created_at.to_rfc3339(),
            updated_at: vm.updated_at.to_rfc3339(),
        })
    }

    async fn validate_targets(&self, targets: &[TargetInput]) -> Result<(), DomainError> {
        for target in targets {
            self.repos.models.get(&target.model_id).await.map_err(|_| {
                DomainError::validation(
                    DomainResource::VirtualModel,
                    format!("target model {} not found", target.model_id),
                )
            })?;
        }
        Ok(())
    }

    pub async fn create(&self, request: CreateVirtualModelRequest) -> Result<VirtualModelDto, DomainError> {
        if request.key.trim().is_empty() {
            return Err(DomainError::validation(DomainResource::VirtualModel, "key is required"));
        }
        self.validate_targets(&request.targets).await?;
        let routing = RoutingStrategy::parse(&request.routing_strategy).unwrap_or(RoutingStrategy::PriorityFailover);
        let vm = self
            .repos
            .virtual_models
            .create(
                NewVirtualModel {
                    key: request.key.clone(),
                    name: request.name.clone(),
                    description: request.description.clone(),
                    routing_strategy: routing,
                    enabled: request.enabled,
                    config: request.config.unwrap_or(json!({})),
                },
                request
                    .targets
                    .iter()
                    .map(|t| NewTarget {
                        model_id: t.model_id.clone(),
                        priority: t.priority.unwrap_or(0),
                        weight: t.weight.unwrap_or(100),
                        enabled: t.enabled,
                        condition: t.condition.clone().unwrap_or(json!({})),
                        overrides: t.overrides.clone().unwrap_or(json!({})),
                    })
                    .collect(),
            )
            .await?;
        audit(&self.repos, "virtual_model.created", "virtual_model", &vm.id, json!({"key": vm.key})).await;
        self.dto(&vm).await
    }

    pub async fn update(&self, id: &str, request: UpdateVirtualModelRequest) -> Result<VirtualModelDto, DomainError> {
        let id = &self.resolve_id(id).await?;
        let routing = request.routing_strategy.as_deref().and_then(RoutingStrategy::parse);
        let vm = self
            .repos
            .virtual_models
            .update(
                id,
                VirtualModelUpdate {
                    name: request.name,
                    description: request.description.map(|d| Some(d)),
                    routing_strategy: routing,
                    enabled: request.enabled,
                    config: request.config,
                },
            )
            .await?;
        audit(&self.repos, "virtual_model.updated", "virtual_model", id, json!({})).await;
        self.dto(&vm).await
    }

    pub async fn replace_targets(&self, id: &str, request: ReplaceTargetsRequest) -> Result<VirtualModelDto, DomainError> {
        let id = &self.resolve_id(id).await?;
        self.validate_targets(&request.targets).await?;
        self.repos
            .virtual_models
            .replace_targets(
                id,
                request
                    .targets
                    .iter()
                    .map(|t| NewTarget {
                        model_id: t.model_id.clone(),
                        priority: t.priority.unwrap_or(0),
                        weight: t.weight.unwrap_or(100),
                        enabled: t.enabled,
                        condition: t.condition.clone().unwrap_or(json!({})),
                        overrides: t.overrides.clone().unwrap_or(json!({})),
                    })
                    .collect(),
            )
            .await?;
        self.get(id).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), DomainError> {
        let id = self.resolve_id(id).await?;
        self.repos.virtual_models.delete(&id).await?;
        audit(&self.repos, "virtual_model.deleted", "virtual_model", &id, json!({})).await;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<VirtualModelDto, DomainError> {
        let id = self.resolve_id(id).await?;
        let vm = self.repos.virtual_models.get(&id).await?;
        self.dto(&vm).await
    }

    pub async fn list(&self) -> Result<Vec<VirtualModelDto>, DomainError> {
        let models = self.repos.virtual_models.list().await?;
        let mut dtos = Vec::new();
        for vm in &models {
            dtos.push(self.dto(vm).await?);
        }
        Ok(dtos)
    }

    /// 路由模拟器（§16.5）：展示候选与被排除原因。
    pub async fn simulate(&self, id: &str) -> Result<RouteSimulation, DomainError> {
        let id = self.resolve_id(id).await?;
        let vm = self.repos.virtual_models.get(&id).await?;
        let targets = self.repos.virtual_models.targets_for(&id).await?;
        let mut candidates = Vec::new();
        let mut selected: Option<String> = None;
        let mut fallback_order = Vec::new();
        for target in &targets {
            let model = self.repos.models.get(&target.model_id).await.ok();
            let provider = match &model {
                Some(m) => self.repos.providers.get(&m.provider_id).await.ok(),
                None => None,
            };
            let label = model
                .as_ref()
                .map(|m| format!("{} ({})", m.display_name, m.model_key))
                .unwrap_or_else(|| format!("missing model {}", target.model_id));
            let mut excluded_reason: Option<String> = None;
            if model.is_none() {
                excluded_reason = Some("model not found".to_string());
            } else if let Some(m) = &model {
                if !m.enabled {
                    excluded_reason = Some("model disabled".to_string());
                }
            }
            if provider.is_none() {
                excluded_reason = Some("provider not found".to_string());
            } else if let Some(p) = &provider {
                if !p.enabled {
                    excluded_reason = Some("provider disabled".to_string());
                }
            }
            let is_selected = excluded_reason.is_none() && target.enabled && selected.is_none();
            if is_selected {
                selected = Some(label.clone());
            }
            if excluded_reason.is_none() && target.enabled {
                fallback_order.push(label.clone());
            }
            candidates.push(CandidateExplanation {
                model_id: target.model_id.clone(),
                label,
                priority: target.priority,
                enabled: target.enabled,
                provider_enabled: provider.map(|p| p.enabled).unwrap_or(false),
                selected: is_selected,
                excluded_reason,
            });
        }
        Ok(RouteSimulation {
            virtual_model: vm.key.clone(),
            candidates,
            selected,
            fallback_order,
        })
    }
}

// ================= Application / API Key =================

fn quota_dto(policy: Option<&QuotaPolicy>) -> Option<QuotaPolicyDto> {
    policy.map(|p| QuotaPolicyDto {
        rpm: p.rpm,
        tpm: p.tpm,
        daily_requests: p.daily_requests,
        monthly_tokens: p.monthly_tokens,
        monthly_cost_microunits: p.monthly_cost_microunits,
        exceed_action: p.exceed_action.clone(),
    })
}

fn quota_values(input: &QuotaInput) -> QuotaValues {
    QuotaValues {
        rpm: input.rpm,
        tpm: input.tpm,
        daily_requests: input.daily_requests,
        monthly_tokens: input.monthly_tokens,
        monthly_cost_microunits: input.monthly_cost_microunits,
        exceed_action: input.exceed_action.clone().unwrap_or_else(|| "block".to_string()),
    }
}

fn key_dto(key: &ApiKey) -> ApiKeyDto {
    ApiKeyDto {
        id: key.id.clone(),
        application_id: key.application_id.clone(),
        name: key.name.clone(),
        prefix: key.prefix.clone(),
        masked_key: format!("aih_live_{}_***", key.prefix),
        scopes: key.scopes.clone(),
        expires_at: key.expires_at.map(|t| t.to_rfc3339()),
        last_used_at: key.last_used_at.map(|t| t.to_rfc3339()),
        revoked_at: key.revoked_at.map(|t| t.to_rfc3339()),
        created_at: key.created_at.to_rfc3339(),
    }
}

pub struct ApplicationService {
    repos: Repos,
}

impl ApplicationService {
    pub fn new(repos: Repos) -> Self {
        Self { repos }
    }

    async fn dto(&self, app: &Application) -> Result<ApplicationDto, DomainError> {
        let quota = self.repos.quota.get_for_subject("application", &app.id).await?;
        let key_count = self.repos.api_keys.count_by_application(&app.id).await?;
        Ok(ApplicationDto {
            id: app.id.clone(),
            key: app.key.clone(),
            name: app.name.clone(),
            status: app.status.clone(),
            allowed_virtual_models: app.allowed_virtual_models.clone(),
            allow_direct_models: app.allow_direct_models,
            monthly_budget_microunits: app.monthly_budget_microunits,
            quota: quota_dto(quota.as_ref()),
            key_count,
            metadata: app.metadata.clone(),
            created_at: app.created_at.to_rfc3339(),
            updated_at: app.updated_at.to_rfc3339(),
        })
    }

    pub async fn create(&self, request: CreateApplicationRequest) -> Result<ApplicationDto, DomainError> {
        if request.key.trim().is_empty() {
            return Err(DomainError::validation(DomainResource::Application, "key is required"));
        }
        let app = self
            .repos
            .applications
            .create(NewApplication {
                key: request.key.clone(),
                name: request.name.clone(),
                status: "active".to_string(),
                allowed_virtual_models: request.allowed_virtual_models.clone().unwrap_or_default(),
                allow_direct_models: request.allow_direct_models.unwrap_or(false),
                monthly_budget_microunits: request.monthly_budget_microunits,
                metadata: json!({}),
            })
            .await?;
        if let Some(quota) = &request.quota {
            self.repos
                .quota
                .upsert_for_subject("application", &app.id, quota_values(quota))
                .await?;
        }
        audit(&self.repos, "application.created", "application", &app.id, json!({"key": app.key})).await;
        self.dto(&app).await
    }

    pub async fn update(&self, id: &str, request: UpdateApplicationRequest) -> Result<ApplicationDto, DomainError> {
        let app = self
            .repos
            .applications
            .update(
                id,
                ApplicationUpdate {
                    name: request.name,
                    status: request.status,
                    allowed_virtual_models: request.allowed_virtual_models,
                    allow_direct_models: request.allow_direct_models,
                    monthly_budget_microunits: request.monthly_budget_microunits.map(Some),
                    metadata: None,
                },
            )
            .await?;
        if let Some(quota) = &request.quota {
            self.repos
                .quota
                .upsert_for_subject("application", id, quota_values(quota))
                .await?;
        }
        audit(&self.repos, "application.updated", "application", id, json!({})).await;
        self.dto(&app).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), DomainError> {
        self.repos.applications.delete(id).await?;
        audit(&self.repos, "application.deleted", "application", id, json!({})).await;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<ApplicationDto, DomainError> {
        let app = self.repos.applications.get(id).await?;
        self.dto(&app).await
    }

    pub async fn list(&self) -> Result<Vec<ApplicationDto>, DomainError> {
        let apps = self.repos.applications.list().await?;
        let mut dtos = Vec::new();
        for app in &apps {
            dtos.push(self.dto(app).await?);
        }
        Ok(dtos)
    }

    /// 创建 API Key：明文仅此一次返回（§9.7/§22.6）。
    pub async fn create_key(&self, application_id: &str, request: CreateApiKeyRequest) -> Result<CreateApiKeyResponse, DomainError> {
        let app = self.repos.applications.get(application_id).await?;
        let (plaintext, prefix) = apikey::generate_key(apikey::KeyEnvironment::Live);
        let key = self
            .repos
            .api_keys
            .create(NewApiKey {
                application_id: app.id.clone(),
                name: request.name,
                prefix,
                secret_hash: apikey::hash_key(&plaintext),
                scopes: request.scopes.unwrap_or_default(),
                expires_at: request
                    .expires_at
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
                    .map(|t| t.with_timezone(&chrono::Utc)),
            })
            .await?;
        audit(
            &self.repos,
            "api_key.created",
            "api_key",
            &key.id,
            json!({"application": app.key}),
        )
        .await;
        Ok(CreateApiKeyResponse {
            key: key_dto(&key),
            plaintext,
        })
    }

    pub async fn list_keys(&self, application_id: &str) -> Result<Vec<ApiKeyDto>, DomainError> {
        let keys = self.repos.api_keys.list_by_application(application_id).await?;
        Ok(keys.iter().map(key_dto).collect())
    }

    pub async fn revoke_key(&self, application_id: &str, key_id: &str) -> Result<(), DomainError> {
        let key = self.repos.api_keys.get(key_id).await?;
        if key.application_id != application_id {
            return Err(DomainError::not_found(DomainResource::ApiKey, key_id));
        }
        self.repos.api_keys.revoke(key_id, now()).await?;
        audit(&self.repos, "api_key.revoked", "api_key", key_id, json!({})).await;
        Ok(())
    }
}

// ================= Requests / Audit 查询 =================

pub struct QueryService {
    repos: Repos,
    resolver: Arc<ModelResolver>,
}

impl QueryService {
    pub fn new(repos: Repos, resolver: Arc<ModelResolver>) -> Self {
        Self { repos, resolver }
    }

    pub async fn list_requests(
        &self,
        application_id: Option<String>,
        status: Option<String>,
        model: Option<String>,
        page: u64,
        page_size: u64,
    ) -> Result<PageResponse<RequestListItem>, DomainError> {
        let (requests, total) = self
            .repos
            .requests
            .list(&RequestFilter {
                application_id,
                status,
                model,
                from: None,
                to: None,
                page,
                page_size,
            })
            .await?;
        let providers = self.repos.providers.list(false).await?;
        let mut items = Vec::new();
        for request in &requests {
            let provider_key = request
                .provider_id
                .as_ref()
                .and_then(|pid| providers.iter().find(|p| &p.id == pid).map(|p| p.key.clone()));
            let usage = self.repos.requests.usage_for_request(&request.id).await.ok().flatten();
            let cost = self.repos.requests.cost_for_request(&request.id).await.ok().flatten();
            let application_key = match &request.application_id {
                Some(app_id) => self.repos.applications.get(app_id).await.ok().map(|a| a.key),
                None => None,
            };
            items.push(RequestListItem {
                id: request.id.clone(),
                trace_id: Some(request.trace_id.clone()),
                application_id: request.application_id.clone(),
                application_key,
                endpoint: request.endpoint.clone(),
                requested_model: request.requested_model.clone(),
                resolved_model_key: request.resolved_model_key.clone(),
                provider_key,
                status: request.status.as_str().to_string(),
                http_status: request.http_status,
                started_at: request.started_at.to_rfc3339(),
                latency_ms: request.latency_ms,
                ttft_ms: request.ttft_ms,
                retry_count: request.retry_count,
                total_tokens: usage.map(|u| u.total_tokens),
                cost_microunits: cost.map(|c| c.total_cost_microunits),
                error_code: request.error_code.clone(),
            });
        }
        Ok(PageResponse {
            data: items,
            meta: PageMeta {
                page,
                page_size,
                total,
            },
        })
    }

    pub async fn request_detail(&self, id: &str) -> Result<RequestDetail, DomainError> {
        let request = self.repos.requests.get(id).await?;
        let usage = self.repos.requests.usage_for_request(id).await?;
        let cost = self.repos.requests.cost_for_request(id).await?;
        let provider_key = match &request.provider_id {
            Some(pid) => self.repos.providers.get(pid).await.ok().map(|p| p.key),
            None => None,
        };
        let application_key = match &request.application_id {
            Some(app_id) => self.repos.applications.get(app_id).await.ok().map(|a| a.key),
            None => None,
        };
        Ok(RequestDetail {
            item: RequestListItem {
                id: request.id.clone(),
                trace_id: Some(request.trace_id.clone()),
                application_id: request.application_id.clone(),
                application_key,
                endpoint: request.endpoint.clone(),
                requested_model: request.requested_model.clone(),
                resolved_model_key: request.resolved_model_key.clone(),
                provider_key,
                status: request.status.as_str().to_string(),
                http_status: request.http_status,
                started_at: request.started_at.to_rfc3339(),
                latency_ms: request.latency_ms,
                ttft_ms: request.ttft_ms,
                retry_count: request.retry_count,
                total_tokens: usage.as_ref().map(|u| u.total_tokens),
                cost_microunits: cost.as_ref().map(|c| c.total_cost_microunits),
                error_code: request.error_code.clone(),
            },
            api_key_id: request.api_key_id.clone(),
            completed_at: request.completed_at.map(|t| t.to_rfc3339()),
            cache_status: request.cache_status.clone(),
            error_message: request.error_message_safe.clone(),
            usage: usage.map(|u| serde_json::to_value(&u).unwrap_or_default()),
            cost: cost.map(|c| serde_json::to_value(&c).unwrap_or_default()),
            metadata: request.metadata.clone(),
        })
    }

    pub async fn list_audit(
        &self,
        event_type: Option<String>,
        resource_type: Option<String>,
        page: u64,
        page_size: u64,
    ) -> Result<PageResponse<AuditEventDto>, DomainError> {
        let (events, total) = self
            .repos
            .audit
            .list(&AuditFilter {
                event_type,
                resource_type,
                actor_id: None,
                trace_id: None,
                page,
                page_size,
            })
            .await?;
        Ok(PageResponse {
            data: events
                .iter()
                .map(|e| AuditEventDto {
                    id: e.id.clone(),
                    trace_id: e.trace_id.clone(),
                    actor_type: e.actor_type.clone(),
                    actor_id: e.actor_id.clone(),
                    event_type: e.event_type.clone(),
                    resource_type: e.resource_type.clone(),
                    resource_id: e.resource_id.clone(),
                    decision: e.decision.clone(),
                    metadata: e.metadata.clone(),
                    created_at: e.created_at.to_rfc3339(),
                })
                .collect(),
            meta: PageMeta {
                page,
                page_size,
                total,
            },
        })
    }
}
