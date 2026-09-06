//! 平台扩展域（M10-M17）：Knowledge / Agent / Tool / MCP / Eval / IAM / 治理策略。
//! 实体字段与 migrations/sqlite/0003_platform.sql 一一对应。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DomainError;

pub type Id = String;
type Result<T> = std::result::Result<T, DomainError>;

// ================= M12 Knowledge =================

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeBase {
    pub id: Id,
    pub key: String,
    pub name: String,
    /// private / department / public
    pub visibility: String,
    pub owner_user_id: Option<Id>,
    pub owner_department_id: Option<Id>,
    pub retrieval_config: Value,
    pub embedding_model_id: Option<Id>,
    /// 预留：rerank 模型（§19.4 Rerank 阶段）
    pub rerank_model_id: Option<Id>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub id: Id,
    pub knowledge_base_id: Id,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub file_hash: String,
    pub storage_key: String,
    /// uploaded / parsing / parse_failed / chunking / embedding / ready / index_failed
    pub parse_status: String,
    pub index_status: String,
    pub parser_version: Option<String>,
    pub error_message: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentChunk {
    pub id: Id,
    pub document_id: Id,
    pub chunk_index: i32,
    pub content: String,
    pub page_no: Option<i32>,
    pub section_path: Option<String>,
    pub token_count: Option<i32>,
    /// V1 本地向量（JSON float array）；Server 模式替换为 pgvector
    pub embedding: Option<Vec<f32>>,
    pub metadata: Value,
}

/// 检索结果（§19.5）：保留 filename/page/section 引用定位
#[derive(Debug, Clone, Serialize)]
pub struct RetrievalHit {
    pub chunk_id: Id,
    pub document_id: Id,
    pub knowledge_base_id: Id,
    pub score: f64,
    pub content: String,
    pub filename: String,
    pub page: Option<i32>,
    pub section: Option<String>,
}

#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    async fn create_kb(&self, kb: NewKnowledgeBase) -> Result<KnowledgeBase>;
    async fn get_kb(&self, id: &str) -> Result<KnowledgeBase>;
    async fn get_kb_by_key(&self, key: &str) -> Result<KnowledgeBase>;
    async fn list_kbs(&self) -> Result<Vec<KnowledgeBase>>;
    async fn delete_kb(&self, id: &str) -> Result<()>;
    async fn update_kb_embedding_model(
        &self,
        id: &str,
        model_id: Option<Id>,
    ) -> Result<KnowledgeBase>;

    async fn create_document(&self, doc: NewDocument) -> Result<Document>;
    async fn get_document(&self, id: &str) -> Result<Document>;
    async fn list_documents(&self, kb_id: &str) -> Result<Vec<Document>>;
    async fn set_document_status(
        &self,
        id: &str,
        parse_status: &str,
        index_status: &str,
        error: Option<String>,
    ) -> Result<()>;
    async fn delete_document(&self, id: &str) -> Result<()>;
    async fn find_document_by_hash(&self, kb_id: &str, hash: &str) -> Result<Option<Document>>;

    async fn replace_chunks(&self, document_id: &str, chunks: Vec<NewChunk>) -> Result<()>;
    async fn list_chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>>;
    async fn delete_chunks(&self, document_id: &str) -> Result<()>;
    async fn update_chunk_embedding(&self, chunk_id: &str, embedding: Vec<f32>) -> Result<()>;
    async fn count_chunks(&self, kb_id: &str) -> Result<i64>;
}

#[derive(Debug, Clone)]
pub struct NewKnowledgeBase {
    pub key: String,
    pub name: String,
    pub visibility: String,
    pub owner_user_id: Option<Id>,
    pub retrieval_config: Value,
    pub embedding_model_id: Option<Id>,
}

#[derive(Debug, Clone)]
pub struct NewDocument {
    pub knowledge_base_id: Id,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub file_hash: String,
    pub storage_key: String,
}

#[derive(Debug, Clone)]
pub struct NewChunk {
    pub chunk_index: i32,
    pub content: String,
    pub page_no: Option<i32>,
    pub section_path: Option<String>,
    pub token_count: Option<i32>,
}

// ================= M14/M15 Agent / Tool / MCP =================

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    /// builtin / http / mcp
    pub kind: String,
    pub input_schema: Value,
    pub config: Value,
    pub timeout_ms: i64,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServer {
    pub id: Id,
    pub key: String,
    pub name: String,
    /// stdio / streamable-http
    pub transport: String,
    pub endpoint_or_command: String,
    pub config: Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentVersion {
    pub id: Id,
    pub agent_id: Id,
    pub version: i32,
    /// draft / published / deprecated
    pub status: String,
    pub model_ref: String,
    pub prompt_version_id: Option<Id>,
    pub system_prompt: Option<String>,
    pub max_steps: i32,
    pub max_tool_calls: i32,
    pub timeout_ms: i64,
    pub max_cost_microunits: Option<i64>,
    pub allowed_tools: Vec<String>,
    pub knowledge_bindings: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRun {
    pub id: Id,
    pub agent_id: Id,
    pub agent_version_id: Id,
    pub trace_id: Option<String>,
    pub status: String,
    pub current_step: i32,
    pub max_steps: i32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub input: Value,
    pub output: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub cost_microunits: i64,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    pub id: Id,
    pub trace_id: Option<String>,
    pub agent_run_id: Option<Id>,
    pub tool_id: Id,
    pub tool_key: String,
    pub status: String,
    pub arguments: Value,
    pub error_code: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub latency_ms: Option<i64>,
}

#[async_trait]
pub trait ToolRepository: Send + Sync {
    async fn create(&self, tool: NewTool) -> Result<Tool>;
    async fn get(&self, id: &str) -> Result<Tool>;
    async fn get_by_key(&self, key: &str) -> Result<Tool>;
    async fn list(&self) -> Result<Vec<Tool>>;
    async fn update(&self, id: &str, update: ToolUpdate) -> Result<Tool>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn list_enabled(&self) -> Result<Vec<Tool>>;
}

#[derive(Debug, Clone)]
pub struct NewTool {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: String,
    pub input_schema: Value,
    pub config: Value,
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ToolUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<Value>,
    pub timeout_ms: Option<i64>,
    pub enabled: Option<bool>,
}

#[async_trait]
pub trait McpServerRepository: Send + Sync {
    async fn create(&self, server: NewMcpServer) -> Result<McpServer>;
    async fn get(&self, id: &str) -> Result<McpServer>;
    async fn get_by_key(&self, key: &str) -> Result<McpServer>;
    async fn list(&self) -> Result<Vec<McpServer>>;
    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct NewMcpServer {
    pub key: String,
    pub name: String,
    pub transport: String,
    pub endpoint_or_command: String,
    pub config: Value,
}

#[async_trait]
pub trait AgentRepository: Send + Sync {
    async fn create(
        &self,
        agent: NewAgent,
        version: NewAgentVersion,
    ) -> Result<(Agent, AgentVersion)>;
    async fn get(&self, id: &str) -> Result<Agent>;
    async fn get_by_key(&self, key: &str) -> Result<Agent>;
    async fn list(&self) -> Result<Vec<Agent>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn create_version(&self, version: NewAgentVersion) -> Result<AgentVersion>;
    async fn versions_for(&self, agent_id: &str) -> Result<Vec<AgentVersion>>;
    async fn get_version(&self, version_id: &str) -> Result<AgentVersion>;
    async fn published_version(&self, agent_id: &str) -> Result<Option<AgentVersion>>;
    async fn set_version_status(&self, version_id: &str, status: &str) -> Result<()>;

    async fn create_run(&self, run: NewAgentRun) -> Result<AgentRun>;
    async fn finish_run(
        &self,
        id: &str,
        status: &str,
        output: Option<Value>,
        error_code: Option<String>,
        error_message: Option<String>,
        cost: i64,
    ) -> Result<()>;
    async fn get_run(&self, id: &str) -> Result<AgentRun>;
    async fn list_runs(&self, agent_id: &str, limit: u64) -> Result<Vec<AgentRun>>;

    async fn insert_tool_call(&self, call: ToolCall) -> Result<()>;
    async fn tool_calls_for_run(&self, run_id: &str) -> Result<Vec<ToolCall>>;
}

#[derive(Debug, Clone)]
pub struct NewAgent {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewAgentVersion {
    pub agent_id: Id,
    pub model_ref: String,
    pub prompt_version_id: Option<Id>,
    pub system_prompt: Option<String>,
    pub max_steps: i32,
    pub max_tool_calls: i32,
    pub timeout_ms: i64,
    pub max_cost_microunits: Option<i64>,
    pub allowed_tools: Vec<String>,
    pub knowledge_bindings: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct NewAgentRun {
    pub agent_id: Id,
    pub agent_version_id: Id,
    pub trace_id: Option<String>,
    pub max_steps: i32,
    pub input: Value,
    pub metadata: Value,
}

// ================= M16 Evaluation =================

#[derive(Debug, Clone, Serialize)]
pub struct EvalDataset {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalCase {
    pub id: Id,
    pub dataset_id: Id,
    pub name: String,
    pub input: Value,
    pub expected_output: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalRun {
    pub id: Id,
    pub dataset_id: Id,
    pub label: String,
    pub candidate: Value,
    pub judge_config: Value,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub id: Id,
    pub run_id: Id,
    pub case_id: Id,
    pub response_text: Option<String>,
    pub latency_ms: Option<i64>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_microunits: i64,
    pub score: Value,
    pub judge: Option<Value>,
}

#[async_trait]
pub trait EvalRepository: Send + Sync {
    async fn create_dataset(
        &self,
        key: &str,
        name: &str,
        description: Option<String>,
    ) -> Result<EvalDataset>;
    async fn get_dataset(&self, id: &str) -> Result<EvalDataset>;
    async fn list_datasets(&self) -> Result<Vec<EvalDataset>>;
    async fn delete_dataset(&self, id: &str) -> Result<()>;
    async fn add_case(
        &self,
        dataset_id: &str,
        name: &str,
        input: Value,
        expected: Option<String>,
    ) -> Result<EvalCase>;
    async fn list_cases(&self, dataset_id: &str) -> Result<Vec<EvalCase>>;
    async fn delete_case(&self, id: &str) -> Result<()>;

    async fn create_run(
        &self,
        dataset_id: &str,
        label: &str,
        candidate: Value,
        judge_config: Value,
    ) -> Result<EvalRun>;
    async fn finish_run(
        &self,
        id: &str,
        status: &str,
        summary: Value,
        error: Option<String>,
    ) -> Result<()>;
    async fn get_run(&self, id: &str) -> Result<EvalRun>;
    async fn list_runs(&self, dataset_id: &str) -> Result<Vec<EvalRun>>;
    async fn insert_result(&self, result: EvalResult) -> Result<()>;
    async fn results_for_run(&self, run_id: &str) -> Result<Vec<EvalResult>>;
}

// ================= M10 IAM（users/roles） =================

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: Id,
    pub external_subject: Option<String>,
    pub identity_provider: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: String,
    pub status: String,
    /// 本地口令哈希（salted sha256）；仅本地 IdP 使用
    pub password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Role {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub system_role: bool,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: NewUser) -> Result<User>;
    async fn get(&self, id: &str) -> Result<User>;
    async fn get_by_username(&self, username: &str) -> Result<Option<User>>;
    async fn get_by_subject(&self, provider: &str, subject: &str) -> Result<Option<User>>;
    async fn list(&self) -> Result<Vec<User>>;
    async fn set_status(&self, id: &str, status: &str) -> Result<()>;
    async fn assign_role(&self, user_id: &str, role_key: &str) -> Result<()>;
    async fn roles_of(&self, user_id: &str) -> Result<Vec<Role>>;
    async fn ensure_role(
        &self,
        key: &str,
        name: &str,
        description: &str,
        permissions: &[&str],
    ) -> Result<Role>;
    async fn list_roles(&self) -> Result<Vec<Role>>;
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub identity_provider: String,
    pub external_subject: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: String,
    pub password_hash: Option<String>,
}

// ================= M17 治理策略（routing_policies / security_policies） =================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicy {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub priority: i32,
    /// {applications?, tasks?, dataClassifications?}
    pub match_rules: Value,
    /// {virtualModel?, allowedProviderKinds?}
    pub action: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub id: Id,
    pub key: String,
    pub name: String,
    /// data_classification / dlp / provider_access / ssrf
    pub policy_type: String,
    pub priority: i32,
    pub rule: Value,
    pub action: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpRule {
    /// 规则名
    pub name: String,
    /// 正则或关键词
    pub pattern: String,
    /// regex / keyword
    pub kind: String,
    /// allow / mask / block
    pub action: String,
}

#[async_trait]
pub trait PolicyRepository: Send + Sync {
    async fn upsert_routing_policy(&self, policy: &RoutingPolicy) -> Result<()>;
    async fn list_routing_policies(&self) -> Result<Vec<RoutingPolicy>>;
    async fn delete_routing_policy(&self, id: &str) -> Result<()>;

    async fn upsert_security_policy(&self, policy: &SecurityPolicy) -> Result<()>;
    async fn list_security_policies(
        &self,
        policy_type: Option<&str>,
    ) -> Result<Vec<SecurityPolicy>>;
    async fn delete_security_policy(&self, id: &str) -> Result<()>;
}
