//! Admin API DTOs（方案 §13.1）。serde 采用 camelCase，与 Web 端 TypeScript 类型一一对应。

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------- Provider ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    pub id: String,
    pub key: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub credential_ref: Option<String>,
    pub credential_configured: bool,
    pub timeout_ms: i64,
    pub max_retries: i32,
    pub enabled: bool,
    pub status: String,
    pub health: String,
    pub last_health_check_at: Option<String>,
    pub model_count: i64,
    pub config: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderRequest {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<i64>,
    #[serde(default)]
    pub max_retries: Option<i32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub config: Value,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<i64>,
    pub max_retries: Option<i32>,
    pub enabled: Option<bool>,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub ok: bool,
    pub latency_ms: Option<i64>,
    pub error: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverModelsResult {
    pub discovered: usize,
    pub created: usize,
    pub updated: usize,
    pub models: Vec<ModelDto>,
}

// ---------- Model ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub id: String,
    pub provider_id: String,
    pub provider_key: Option<String>,
    pub provider_name: Option<String>,
    pub model_key: String,
    pub display_name: String,
    pub model_type: String,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub capabilities: Value,
    pub pricing: Value,
    pub enabled: bool,
    pub discovered: bool,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelRequest {
    pub provider_id: String,
    pub model_key: String,
    pub display_name: String,
    #[serde(default = "default_model_type")]
    pub model_type: String,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
    #[serde(default)]
    pub capabilities: Option<Value>,
    #[serde(default)]
    pub pricing: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_model_type() -> String {
    "chat".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateModelRequest {
    pub display_name: Option<String>,
    pub model_type: Option<String>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub capabilities: Option<Value>,
    pub pricing: Option<Value>,
    pub enabled: Option<bool>,
    pub metadata: Option<Value>,
}

// ---------- Virtual Model ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualModelTargetDto {
    pub id: String,
    pub model_id: String,
    pub model_label: Option<String>,
    pub priority: i32,
    pub weight: i32,
    pub enabled: bool,
    pub condition: Value,
    pub overrides: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualModelDto {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub routing_strategy: String,
    pub enabled: bool,
    pub config: Value,
    pub targets: Vec<VirtualModelTargetDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVirtualModelRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_routing_strategy")]
    pub routing_strategy: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub targets: Vec<TargetInput>,
}

fn default_routing_strategy() -> String {
    "priority_failover".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVirtualModelRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub routing_strategy: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInput {
    pub model_id: String,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub weight: Option<i32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub condition: Option<Value>,
    #[serde(default)]
    pub overrides: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceTargetsRequest {
    pub targets: Vec<TargetInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulation {
    pub virtual_model: String,
    pub candidates: Vec<CandidateExplanation>,
    pub selected: Option<String>,
    pub fallback_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateExplanation {
    pub model_id: String,
    pub label: String,
    pub priority: i32,
    pub enabled: bool,
    pub provider_enabled: bool,
    pub selected: bool,
    pub excluded_reason: Option<String>,
}

// ---------- Application / API Key ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationDto {
    pub id: String,
    pub key: String,
    pub name: String,
    pub status: String,
    pub allowed_virtual_models: Vec<String>,
    pub allow_direct_models: bool,
    pub monthly_budget_microunits: Option<i64>,
    pub quota: Option<QuotaPolicyDto>,
    pub key_count: i64,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaPolicyDto {
    pub rpm: Option<i64>,
    pub tpm: Option<i64>,
    pub daily_requests: Option<i64>,
    pub monthly_tokens: Option<i64>,
    pub monthly_cost_microunits: Option<i64>,
    pub exceed_action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApplicationRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub allowed_virtual_models: Option<Vec<String>>,
    #[serde(default)]
    pub allow_direct_models: Option<bool>,
    #[serde(default)]
    pub monthly_budget_microunits: Option<i64>,
    #[serde(default)]
    pub quota: Option<QuotaInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaInput {
    pub rpm: Option<i64>,
    pub tpm: Option<i64>,
    pub daily_requests: Option<i64>,
    pub monthly_tokens: Option<i64>,
    pub monthly_cost_microunits: Option<i64>,
    pub exceed_action: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApplicationRequest {
    pub name: Option<String>,
    pub status: Option<String>,
    pub allowed_virtual_models: Option<Vec<String>>,
    pub allow_direct_models: Option<bool>,
    pub monthly_budget_microunits: Option<i64>,
    pub quota: Option<QuotaInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyDto {
    pub id: String,
    pub application_id: String,
    pub name: String,
    pub prefix: String,
    pub masked_key: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyRequest {
    #[serde(default = "default_key_name")]
    pub name: String,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

fn default_key_name() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyResponse {
    pub key: ApiKeyDto,
    /// 明文仅此一次返回（方案 §9.7 / §22.6）
    pub plaintext: String,
}

// ---------- Usage / Requests / Audit ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub requests: i64,
    pub success_requests: i64,
    pub failed_requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cost_microunits: i64,
    pub currency: String,
    pub success_rate: f64,
    pub p50_latency_ms: Option<i64>,
    pub p95_latency_ms: Option<i64>,
    pub avg_ttft_ms: Option<i64>,
    pub cache_hit_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeseriesPoint {
    pub bucket: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_microunits: i64,
    pub errors: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupUsage {
    pub group: String,
    pub requests: i64,
    pub total_tokens: i64,
    pub cost_microunits: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestListItem {
    pub id: String,
    pub trace_id: Option<String>,
    pub application_id: Option<String>,
    pub application_key: Option<String>,
    pub endpoint: String,
    pub requested_model: String,
    pub resolved_model_key: Option<String>,
    pub provider_key: Option<String>,
    pub status: String,
    pub http_status: Option<i64>,
    pub started_at: String,
    pub latency_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub retry_count: i32,
    pub total_tokens: Option<i64>,
    pub cost_microunits: Option<i64>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetail {
    #[serde(flatten)]
    pub item: RequestListItem,
    pub api_key_id: Option<String>,
    pub completed_at: Option<String>,
    pub cache_status: Option<String>,
    pub error_message: Option<String>,
    pub usage: Option<Value>,
    pub cost: Option<Value>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventDto {
    pub id: String,
    pub trace_id: Option<String>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub decision: Option<String>,
    pub metadata: Value,
    pub created_at: String,
}

// ---------- System ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub version: String,
    pub mode: String,
    pub gateway_endpoint: String,
    pub db_driver: String,
    pub started_at: String,
}

// ---------- Playground ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundRunRequest {
    pub model: String,
    pub messages: Vec<Value>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}
