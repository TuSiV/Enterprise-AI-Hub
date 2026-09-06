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

//! Provider Adapter 统一协议（方案 §10）：Gateway 只依赖此 trait 与统一错误类别，
//! 厂商特有协议只能存在于具体 Adapter crate 内（§35.2 反模式禁止 if/else 写进 Gateway）。

use std::sync::Arc;

use aihub_domain::canonical::{
    CanonicalChatRequest, CanonicalChatResponse, CanonicalEmbeddingRequest,
    CanonicalEmbeddingResponse, DiscoveredModel, StreamEvent,
};
use aihub_domain::entities::{Provider, ProviderKind};
use aihub_secrets::SecretValue;

pub use aihub_domain::error::ErrorCategory;

/// 流式事件流：Adapter 必须将各厂商 SSE/event 结构映射成统一 StreamEvent（§10.5）。
pub type ProviderStream = std::pin::Pin<Box<dyn futures::Stream<Item = StreamEvent> + Send>>;

#[derive(Debug, thiserror::Error)]
#[error("{category:?}: {message}")]
pub struct ProviderError {
    pub category: ErrorCategory,
    pub message: String,
    pub http_status: Option<u16>,
    pub retry_after_ms: Option<u64>,
}

impl ProviderError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            http_status: None,
            retry_after_ms: None,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_retry_after(mut self, ms: u64) -> Self {
        self.retry_after_ms = Some(ms);
        self
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Debug, Clone)]
pub struct ProviderHealth {
    pub status: String, // healthy | degraded | unavailable
    pub latency_ms: Option<i64>,
    pub http_status: Option<u16>,
    pub error_category: Option<ErrorCategory>,
    pub message: Option<String>,
}

/// 统一 Provider 接口（方案 §10.2）。
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    async fn list_models(&self) -> ProviderResult<Vec<DiscoveredModel>>;

    async fn chat(&self, request: CanonicalChatRequest) -> ProviderResult<CanonicalChatResponse>;

    async fn chat_stream(&self, request: CanonicalChatRequest) -> ProviderResult<ProviderStream>;

    async fn embeddings(
        &self,
        request: CanonicalEmbeddingRequest,
    ) -> ProviderResult<CanonicalEmbeddingResponse>;

    async fn health_check(&self) -> ProviderResult<ProviderHealth>;
}

/// Adapter 构建所需的运行时配置（来自 Provider 行 + SecretStore 解出的凭据）。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeConfig {
    pub provider_id: String,
    pub provider_key: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub credential: Option<SecretValue>,
    pub proxy_url: Option<String>,
    pub timeout_ms: i64,
    pub max_retries: i32,
    pub config: serde_json::Value,
}

impl ProviderRuntimeConfig {
    pub fn from_provider(provider: &Provider, credential: Option<SecretValue>) -> Self {
        Self {
            provider_id: provider.id.clone(),
            provider_key: provider.key.clone(),
            kind: provider.kind,
            base_url: provider.base_url.clone(),
            credential,
            proxy_url: provider.proxy_url.clone(),
            timeout_ms: provider.timeout_ms,
            max_retries: provider.max_retries,
            config: provider.config.clone(),
        }
    }
}

/// ProviderFactory：由具体 Adapter crate 实现；application 层通过工厂构建实例，
/// 避免依赖具体 Adapter 类型。
#[async_trait::async_trait]
pub trait ProviderFactory: Send + Sync {
    /// 支持的协议标识（如 openai_compatible）。
    fn protocol(&self) -> &'static str;

    fn build(&self, config: ProviderRuntimeConfig) -> ProviderResult<Arc<dyn ModelProvider>>;
}
