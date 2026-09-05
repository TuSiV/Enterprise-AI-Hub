//! Provider Registry：按需构建并缓存 Provider Adapter 实例。
//! 凭据在构建时从 SecretStore 解出；Provider 更新后通过指纹失效重建。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use aihub_domain::entities::Provider;
use aihub_domain::error::{DomainError, DomainResource};
use aihub_domain::repos::ProviderRepository;
use aihub_provider_core::{ProviderError, ProviderFactory, ProviderRuntimeConfig};
use aihub_secrets::{SecretRef, SecretStore};

type ModelProviderArc = Arc<dyn aihub_provider_core::ModelProvider>;

struct CachedProvider {
    fingerprint: String,
    provider: ModelProviderArc,
}

pub struct ProviderRegistry {
    factories: Vec<Arc<dyn ProviderFactory>>,
    cache: RwLock<HashMap<String, CachedProvider>>,
    providers: Arc<dyn ProviderRepository>,
    secrets: Arc<dyn SecretStore>,
}

impl ProviderRegistry {
    pub fn new(
        factories: Vec<Arc<dyn ProviderFactory>>,
        providers: Arc<dyn ProviderRepository>,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            factories,
            cache: RwLock::new(HashMap::new()),
            providers,
            secrets,
        }
    }

    pub fn supported_protocols(&self) -> Vec<&'static str> {
        self.factories.iter().map(|f| f.protocol()).collect()
    }

    async fn resolve_credential(
        &self,
        provider: &Provider,
    ) -> Result<Option<aihub_secrets::SecretValue>, ProviderError> {
        let Some(credential_ref) = &provider.credential_ref else {
            return Ok(None);
        };
        match self.secrets.get(&SecretRef(credential_ref.clone())).await {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => Ok(None),
            Err(e) => Err(ProviderError::new(
                aihub_domain::error::ErrorCategory::Connection,
                format!("failed to load credential from secret store: {e}"),
            )),
        }
    }

    /// 获取（或构建）指定 Provider 的 Adapter 实例。
    pub async fn get(&self, provider_id: &str) -> Result<ModelProviderArc, DomainError> {
        let provider = self.providers.get(provider_id).await?;
        self.get_for(&provider).await
    }

    pub async fn get_for(&self, provider: &Provider) -> Result<ModelProviderArc, DomainError> {
        let fingerprint = format!(
            "{}|{}|{}|{}|{}",
            provider.base_url,
            provider.timeout_ms,
            provider.max_retries,
            provider.config,
            provider.updated_at.to_rfc3339()
        );
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&provider.id) {
                if cached.fingerprint == fingerprint {
                    return Ok(cached.provider.clone());
                }
            }
        }

        let protocol = provider.adapter_protocol();
        let factory = self
            .factories
            .iter()
            .find(|f| f.protocol() == protocol)
            .ok_or_else(|| {
                DomainError::validation(
                    DomainResource::Provider,
                    format!("no adapter registered for provider protocol '{protocol}'"),
                )
            })?;

        let credential = self.resolve_credential(provider).await.map_err(|e| {
            DomainError::internal(
                DomainResource::Provider,
                format!("secret resolution failed: {e}"),
            )
        })?;
        let runtime_config = ProviderRuntimeConfig::from_provider(provider, credential);
        let built = factory.build(runtime_config).map_err(|e| {
            DomainError::internal(
                DomainResource::Provider,
                format!("adapter build failed: {e}"),
            )
        })?;
        self.cache.write().await.insert(
            provider.id.clone(),
            CachedProvider {
                fingerprint,
                provider: built.clone(),
            },
        );
        Ok(built)
    }

    pub async fn invalidate(&self, provider_id: &str) {
        self.cache.write().await.remove(provider_id);
    }

    pub async fn invalidate_all(&self) {
        self.cache.write().await.clear();
    }
}
