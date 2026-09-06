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

//! Repository Ports（方案 §21.1）：由 persistence Adapter 实现。
//! 高频 append-only 数据（requests/usage/cost/audit）不设计成复杂聚合关系（§8.2）。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::cost::Pricing;
use crate::entities::*;
use crate::error::DomainError;

type Result<T> = std::result::Result<T, DomainError>;

#[derive(Debug, Clone, Default)]
pub struct ModelFilter {
    pub provider_id: Option<Id>,
    pub model_type: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelUpdate {
    pub display_name: Option<String>,
    pub model_type: Option<ModelType>,
    pub context_window: Option<Option<i64>>,
    pub max_output_tokens: Option<Option<i64>>,
    pub capabilities: Option<Value>,
    pub pricing: Option<Pricing>,
    pub enabled: Option<bool>,
    pub metadata: Option<Value>,
}

#[async_trait]
pub trait ModelRepository: Send + Sync {
    async fn create(&self, model: NewModel) -> Result<Model>;
    async fn upsert_discovered(&self, model: NewModel) -> Result<Model>;
    async fn get(&self, id: &str) -> Result<Model>;
    async fn list(&self, filter: &ModelFilter) -> Result<Vec<Model>>;
    async fn update(&self, id: &str, update: ModelUpdate) -> Result<Model>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn count_by_provider(&self, provider_id: &str) -> Result<i64>;
    async fn count_enabled(&self) -> Result<i64>;
}

#[derive(Debug, Clone)]
pub struct NewModel {
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
}

#[derive(Debug, Clone, Default)]
pub struct ProviderUpdate {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub credential_configured: Option<bool>,
    pub proxy_url: Option<Option<String>>,
    pub timeout_ms: Option<i64>,
    pub max_retries: Option<i32>,
    pub enabled: Option<bool>,
    pub status: Option<String>,
    pub config: Option<Value>,
    pub credential_ref: Option<String>,
}

#[async_trait]
pub trait ProviderRepository: Send + Sync {
    async fn create(&self, provider: NewProvider) -> Result<Provider>;
    async fn get(&self, id: &str) -> Result<Provider>;
    async fn get_by_key(&self, key: &str) -> Result<Provider>;
    async fn list(&self, enabled_only: bool) -> Result<Vec<Provider>>;
    async fn update(&self, id: &str, update: ProviderUpdate) -> Result<Provider>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn set_health(&self, id: &str, health: &str, checked_at: DateTime<Utc>) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct NewProvider {
    pub key: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub proxy_url: Option<String>,
    pub timeout_ms: i64,
    pub max_retries: i32,
    pub enabled: bool,
    pub status: String,
    pub config: Value,
}

#[async_trait]
pub trait ProviderHealthRepository: Send + Sync {
    async fn insert_sample(&self, sample: NewHealthSample) -> Result<()>;
    async fn latest_per_provider(&self) -> Result<Vec<ProviderHealthSample>>;
}

#[derive(Debug, Clone)]
pub struct NewHealthSample {
    pub provider_id: Id,
    pub model_id: Option<Id>,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub http_status: Option<i64>,
    pub error_category: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VirtualModelUpdate {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub routing_strategy: Option<RoutingStrategy>,
    pub enabled: Option<bool>,
    pub config: Option<Value>,
}

#[async_trait]
pub trait VirtualModelRepository: Send + Sync {
    async fn create(&self, vm: NewVirtualModel, targets: Vec<NewTarget>) -> Result<VirtualModel>;
    async fn get(&self, id: &str) -> Result<VirtualModel>;
    async fn get_by_key(&self, key: &str) -> Result<VirtualModel>;
    async fn list(&self) -> Result<Vec<VirtualModel>>;
    async fn update(&self, id: &str, update: VirtualModelUpdate) -> Result<VirtualModel>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn replace_targets(
        &self,
        virtual_model_id: &str,
        targets: Vec<NewTarget>,
    ) -> Result<Vec<VirtualModelTarget>>;
    async fn targets_for(&self, virtual_model_id: &str) -> Result<Vec<VirtualModelTarget>>;
    async fn list_all_targets(&self) -> Result<Vec<VirtualModelTarget>>;
}

#[derive(Debug, Clone)]
pub struct NewVirtualModel {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub routing_strategy: RoutingStrategy,
    pub enabled: bool,
    pub config: Value,
}

#[derive(Debug, Clone)]
pub struct NewTarget {
    pub model_id: Id,
    pub priority: i32,
    pub weight: i32,
    pub enabled: bool,
    pub condition: Value,
    pub overrides: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ApplicationUpdate {
    pub name: Option<String>,
    pub status: Option<String>,
    pub allowed_virtual_models: Option<Vec<String>>,
    pub allow_direct_models: Option<bool>,
    pub monthly_budget_microunits: Option<Option<i64>>,
    pub metadata: Option<Value>,
}

#[async_trait]
pub trait ApplicationRepository: Send + Sync {
    async fn create(&self, app: NewApplication) -> Result<Application>;
    async fn get(&self, id: &str) -> Result<Application>;
    async fn get_by_key(&self, key: &str) -> Result<Application>;
    async fn list(&self) -> Result<Vec<Application>>;
    async fn update(&self, id: &str, update: ApplicationUpdate) -> Result<Application>;
    async fn delete(&self, id: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct NewApplication {
    pub key: String,
    pub name: String,
    pub status: String,
    pub allowed_virtual_models: Vec<String>,
    pub allow_direct_models: bool,
    pub monthly_budget_microunits: Option<i64>,
    pub metadata: Value,
}

#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn create(&self, key: NewApiKey) -> Result<ApiKey>;
    async fn get(&self, id: &str) -> Result<ApiKey>;
    async fn get_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>>;
    async fn list_by_application(&self, application_id: &str) -> Result<Vec<ApiKey>>;
    async fn revoke(&self, id: &str, at: DateTime<Utc>) -> Result<()>;
    async fn touch_last_used(&self, id: &str, at: DateTime<Utc>) -> Result<()>;
    async fn count_by_application(&self, application_id: &str) -> Result<i64>;
}

#[derive(Debug, Clone)]
pub struct NewApiKey {
    pub application_id: Id,
    pub name: String,
    pub prefix: String,
    pub secret_hash: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait QuotaRepository: Send + Sync {
    async fn upsert_for_subject(
        &self,
        subject_type: &str,
        subject_id: &str,
        policy: QuotaValues,
    ) -> Result<QuotaPolicy>;
    async fn get_for_subject(
        &self,
        subject_type: &str,
        subject_id: &str,
    ) -> Result<Option<QuotaPolicy>>;
}

#[derive(Debug, Clone, Default)]
pub struct QuotaValues {
    pub rpm: Option<i64>,
    pub tpm: Option<i64>,
    pub daily_requests: Option<i64>,
    pub monthly_tokens: Option<i64>,
    pub monthly_cost_microunits: Option<i64>,
    pub exceed_action: String,
}

#[derive(Debug, Clone, Default)]
pub struct RequestFilter {
    pub application_id: Option<Id>,
    pub status: Option<String>,
    pub model: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Clone)]
pub struct RequestFinish {
    pub status: RequestStatus,
    pub http_status: Option<i64>,
    pub completed_at: DateTime<Utc>,
    pub ttft_ms: Option<i64>,
    pub latency_ms: Option<i64>,
    pub retry_count: i32,
    pub error_code: Option<String>,
    pub error_message_safe: Option<String>,
    pub resolved_model_id: Option<Id>,
    pub resolved_model_key: Option<String>,
    pub provider_id: Option<Id>,
}

#[async_trait]
pub trait RequestRepository: Send + Sync {
    async fn create(&self, request: NewAiRequest) -> Result<AiRequest>;
    async fn finish(&self, id: &str, finish: RequestFinish) -> Result<()>;
    async fn get(&self, id: &str) -> Result<AiRequest>;
    async fn list(&self, filter: &RequestFilter) -> Result<(Vec<AiRequest>, u64)>;
    async fn insert_usage(&self, usage: UsageRecord) -> Result<()>;
    async fn insert_cost(&self, cost: CostRecord) -> Result<()>;
    async fn usage_for_request(&self, request_id: &str) -> Result<Option<UsageRecord>>;
    async fn cost_for_request(&self, request_id: &str) -> Result<Option<CostRecord>>;
    async fn update_resolved(
        &self,
        id: &str,
        model_id: &str,
        model_key: &str,
        provider_id: &str,
    ) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct NewAiRequest {
    pub id: Id,
    pub trace_id: String,
    pub application_id: Option<Id>,
    pub user_id: Option<Id>,
    pub api_key_id: Option<Id>,
    pub endpoint: String,
    pub requested_model: String,
    pub metadata: Value,
    pub started_at: DateTime<Utc>,
}

/// Usage/Cost 聚合查询结果（方案 §13.1 usage/cost 接口）
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct UsageAggregates {
    pub requests: i64,
    pub success_requests: i64,
    pub failed_requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
    pub cost_microunits: i64,
    pub p50_latency_ms: Option<i64>,
    pub p95_latency_ms: Option<i64>,
    pub avg_ttft_ms: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct UsageQuery {
    pub application_id: Option<Id>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait UsageRepository: Send + Sync {
    async fn summary(&self, query: &UsageQuery) -> Result<UsageAggregates>;
    async fn timeseries(
        &self,
        query: &UsageQuery,
        bucket: &str,
    ) -> Result<Vec<(String, UsageAggregates)>>;
    async fn by_model(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>>;
    async fn by_application(&self, query: &UsageQuery) -> Result<Vec<(String, UsageAggregates)>>;
    async fn monthly_cost_for_application(
        &self,
        application_id: &str,
        month_start: DateTime<Utc>,
    ) -> Result<i64>;
    async fn monthly_tokens_for_application(
        &self,
        application_id: &str,
        month_start: DateTime<Utc>,
    ) -> Result<i64>;
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub event_type: Option<String>,
    pub resource_type: Option<String>,
    pub actor_id: Option<String>,
    pub trace_id: Option<String>,
    pub page: u64,
    pub page_size: u64,
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn insert(&self, event: AuditEvent) -> Result<()>;
    async fn list(&self, filter: &AuditFilter) -> Result<(Vec<AuditEvent>, u64)>;
}
