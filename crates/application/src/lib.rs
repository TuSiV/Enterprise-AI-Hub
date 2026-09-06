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

//! Application 层：用例服务 + Gateway 执行流水线（方案 §12）。
//! 依赖 Domain 与 Port，不依赖具体数据库/HTTP 实现。

pub mod agent_service;
pub mod apikey;
pub mod breaker;
pub mod error;
pub mod eval_service;
pub mod iam_service;
pub mod knowledge_service;
pub mod limiter;
pub mod pipeline;
pub mod playground;
pub mod policy_service;
pub mod prompt_service;
pub mod registry;
pub mod resolver;
pub mod s3_storage;
pub mod seed;
pub mod services;

pub use apikey::{generate_key, hash_key, key_prefix, KeyEnvironment};
pub use breaker::{BreakerDecision, CircuitBreakerRegistry};
pub use error::{GatewayCode, PipelineError};
pub use limiter::{LimiterDecision, RateLimiter};
pub use pipeline::{AuthContext, ChatExecution, ChatPipeline, PipelineStreamEvent, UsageCost};
pub use registry::ProviderRegistry;
pub use resolver::ModelResolver;

use aihub_domain::platform::JobRepository;
use aihub_domain::platform::*;
use aihub_domain::prompt::PromptRepository;
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
    pub prompts: Arc<dyn PromptRepository>,
    pub users: Arc<dyn UserRepository>,
    pub knowledge: Arc<dyn KnowledgeRepository>,
    pub tools: Arc<dyn ToolRepository>,
    pub mcp_servers: Arc<dyn McpServerRepository>,
    pub agents: Arc<dyn AgentRepository>,
    pub evals: Arc<dyn EvalRepository>,
    pub policies: Arc<dyn PolicyRepository>,
    pub jobs: Arc<dyn JobRepository>,
}

pub const PLAYGROUND_APPLICATION_KEY: &str = "local-playground";
