//! Application 层：用例服务 + Gateway 执行流水线（方案 §12）。
//! 依赖 Domain 与 Port，不依赖具体数据库/HTTP 实现。

pub mod apikey;
pub mod breaker;
pub mod error;
pub mod limiter;
pub mod pipeline;
pub mod playground;
pub mod registry;
pub mod resolver;
pub mod seed;
pub mod services;

pub use apikey::{generate_key, hash_key, key_prefix, KeyEnvironment};
pub use breaker::{BreakerDecision, CircuitBreakerRegistry};
pub use error::{GatewayCode, PipelineError};
pub use limiter::{LimiterDecision, RateLimiter};
pub use pipeline::{AuthContext, ChatExecution, ChatPipeline, PipelineStreamEvent, UsageCost};
pub use registry::ProviderRegistry;
pub use resolver::ModelResolver;

use aihub_domain::repos::*;
use std::sync::Arc;

/// 组合所有仓储的应用状态容器，由 server/desktop host 装配。
#[derive(Clone)]
pub struct Repos {
    pub providers: Arc<dyn ProviderRepository>,
    pub provider_health: Arc<dyn ProviderHealthRepository>,
    pub models: Arc<dyn ModelRepository>,
    pub virtual_models: Arc<dyn VirtualModelRepository>,
    pub applications: Arc<dyn ApplicationRepository>,
    pub api_keys: Arc<dyn ApiKeyRepository>,
    pub quota: Arc<dyn QuotaRepository>,
    pub requests: Arc<dyn RequestRepository>,
    pub usage: Arc<dyn UsageRepository>,
    pub audit: Arc<dyn AuditRepository>,
}

pub const PLAYGROUND_APPLICATION_KEY: &str = "local-playground";
