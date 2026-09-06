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

//! 核心实体（方案 §8 / §9）。字段与数据库 schema 一一对应；
//! 金额一律使用整数 microunits（1 USD = 1_000_000 microunits），不使用浮点存储。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cost::Pricing;

pub type Id = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAI,
    Anthropic,
    Gemini,
    OpenAICompatible,
    Ollama,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::OpenAI => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Gemini => "gemini",
            ProviderKind::OpenAICompatible => "openai_compatible",
            ProviderKind::Ollama => "ollama",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "openai" => Some(ProviderKind::OpenAI),
            "anthropic" => Some(ProviderKind::Anthropic),
            "gemini" => Some(ProviderKind::Gemini),
            "openai_compatible" => Some(ProviderKind::OpenAICompatible),
            "ollama" => Some(ProviderKind::Ollama),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Provider {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub credential_ref: Option<String>,
    /// SecretStore 中是否已配置凭据（落库缓存，避免读取 SecretStore 本身）
    pub credential_configured: bool,
    pub proxy_url: Option<String>,
    pub timeout_ms: i64,
    pub max_retries: i32,
    pub enabled: bool,
    /// 生命周期：draft | active | disabled（方案 §16.1 / §39.1）
    pub status: String,
    /// 健康：unknown | healthy | degraded | unavailable
    pub health: String,
    pub last_health_check_at: Option<DateTime<Utc>>,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Provider {
    /// OpenAI / Ollama / openai_compatible 全部复用 OpenAI-compatible Adapter。
    pub fn adapter_protocol(&self) -> &'static str {
        match self.kind {
            ProviderKind::OpenAI | ProviderKind::OpenAICompatible | ProviderKind::Ollama => {
                "openai_compatible"
            }
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    Chat,
    Reasoning,
    Embedding,
    Rerank,
    Image,
    Multimodal,
}

impl ModelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelType::Chat => "chat",
            ModelType::Reasoning => "reasoning",
            ModelType::Embedding => "embedding",
            ModelType::Rerank => "rerank",
            ModelType::Image => "image",
            ModelType::Multimodal => "multimodal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(ModelType::Chat),
            "reasoning" => Some(ModelType::Reasoning),
            "embedding" => Some(ModelType::Embedding),
            "rerank" => Some(ModelType::Rerank),
            "image" => Some(ModelType::Image),
            "multimodal" => Some(ModelType::Multimodal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub id: Id,
    pub provider_id: Id,
    pub model_key: String,
    pub display_name: String,
    pub model_type: ModelType,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub capabilities: Value,
    pub pricing: Pricing,
    pub enabled: bool,
    pub discovered: bool,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    PriorityFailover,
    Weighted,
    LowestCost,
    LowestLatency,
}

impl RoutingStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RoutingStrategy::PriorityFailover => "priority_failover",
            RoutingStrategy::Weighted => "weighted",
            RoutingStrategy::LowestCost => "lowest_cost",
            RoutingStrategy::LowestLatency => "lowest_latency",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "priority_failover" => Some(RoutingStrategy::PriorityFailover),
            "weighted" => Some(RoutingStrategy::Weighted),
            "lowest_cost" => Some(RoutingStrategy::LowestCost),
            "lowest_latency" => Some(RoutingStrategy::LowestLatency),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VirtualModel {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub routing_strategy: RoutingStrategy,
    pub enabled: bool,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VirtualModelTarget {
    pub id: Id,
    pub virtual_model_id: Id,
    pub model_id: Id,
    pub priority: i32,
    pub weight: i32,
    pub enabled: bool,
    pub condition: Value,
    pub overrides: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Application {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub status: String,
    /// 允许的 Virtual Model key 列表；空数组表示允许全部。
    pub allowed_virtual_models: Vec<String>,
    pub allow_direct_models: bool,
    pub monthly_budget_microunits: Option<i64>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKey {
    pub id: Id,
    pub application_id: Id,
    pub name: String,
    pub prefix: String,
    pub secret_hash: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        match self.expires_at {
            Some(expires) => expires > now,
            None => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Accepted,
    Routing,
    Running,
    Completed,
    Failed,
    ClientCancelled,
    Timeout,
}

impl RequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestStatus::Accepted => "accepted",
            RequestStatus::Routing => "routing",
            RequestStatus::Running => "running",
            RequestStatus::Completed => "completed",
            RequestStatus::Failed => "failed",
            RequestStatus::ClientCancelled => "client_cancelled",
            RequestStatus::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AiRequest {
    pub id: Id,
    pub trace_id: String,
    pub application_id: Option<Id>,
    pub user_id: Option<Id>,
    pub api_key_id: Option<Id>,
    pub endpoint: String,
    pub requested_model: String,
    pub resolved_model_id: Option<Id>,
    pub resolved_model_key: Option<String>,
    pub provider_id: Option<Id>,
    pub status: RequestStatus,
    pub http_status: Option<i64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub ttft_ms: Option<i64>,
    pub latency_ms: Option<i64>,
    pub retry_count: i32,
    pub cache_status: Option<String>,
    pub error_code: Option<String>,
    pub error_message_safe: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageRecord {
    pub id: Id,
    pub request_id: Id,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub usage_source: String,
    pub raw_usage: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostRecord {
    pub id: Id,
    pub request_id: Id,
    pub currency: String,
    pub input_cost_microunits: i64,
    pub output_cost_microunits: i64,
    pub cache_cost_microunits: i64,
    pub reasoning_cost_microunits: i64,
    pub total_cost_microunits: i64,
    pub pricing_snapshot: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub id: Id,
    pub trace_id: Option<String>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub decision: Option<String>,
    pub payload_ref: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaPolicy {
    pub id: Id,
    pub subject_type: String,
    pub subject_id: String,
    pub rpm: Option<i64>,
    pub tpm: Option<i64>,
    pub daily_requests: Option<i64>,
    pub monthly_tokens: Option<i64>,
    pub monthly_cost_microunits: Option<i64>,
    pub exceed_action: String,
    pub fallback_virtual_model_id: Option<Id>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealthSample {
    pub id: Id,
    pub provider_id: Id,
    pub model_id: Option<Id>,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub http_status: Option<i64>,
    pub error_category: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Page {
    pub page: u64,
    pub page_size: u64,
}
