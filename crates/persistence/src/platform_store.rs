//! Knowledge（M12）/ Tool+MCP+Agent（M14/M15）/ Eval（M16）/ Policy（M17）的 SQLite 仓储。

use aihub_domain::error::DomainError;
use aihub_domain::platform::*;
use serde_json::Value;

type Result<T> = std::result::Result<T, DomainError>;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts};

fn now() -> String {
    now_rfc3339()
}

// ================= Knowledge =================

pub struct SqliteKnowledgeRepository {
    pool: SqlitePool,
}

impl SqliteKnowledgeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn kb_from_row(row: &sqlx::sqlite::SqliteRow) -> KnowledgeBase {
    KnowledgeBase {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        visibility: row.get("visibility"),
        owner_user_id: row.get("owner_user_id"),
        owner_department_id: row.get("owner_department_id"),
        retrieval_config: parse_json(Some(row.get("retrieval_config_json"))),
        embedding_model_id: row.get("embedding_model_id"),
        status: row.get("status"),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn doc_from_row(row: &sqlx::sqlite::SqliteRow) -> Document {
    Document {
        id: row.get("id"),
        knowledge_base_id: row.get("knowledge_base_id"),
        filename: row.get("filename"),
        mime_type: row.get("mime_type"),
        size_bytes: row.get("size_bytes"),
        file_hash: row.get("file_hash"),
        storage_key: row.get("storage_key"),
        parse_status: row.get("parse_status"),
        index_status: row.get("index_status"),
        parser_version: row.get("parser_version"),
        error_message: row.get("error_message"),
        metadata: parse_json(Some(row.get("metadata_json"))),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn chunk_from_row(row: &sqlx::sqlite::SqliteRow) -> DocumentChunk {
    DocumentChunk {
        id: row.get("id"),
        document_id: row.get("document_id"),
        chunk_index: row.get::<i64, _>("chunk_index") as i32,
        content: row.get("content"),
        page_no: row.get("page_no"),
        section_path: row.get("section_path"),
        token_count: row.get("token_count"),
        embedding: row
            .get::<Option<String>, _>("embedding_json")
            .and_then(|s| serde_json::from_str(&s).ok()),
        metadata: parse_json(Some(row.get("metadata_json"))),
    }
}

#[async_trait]
impl KnowledgeRepository for SqliteKnowledgeRepository {
    async fn create_kb(&self, kb: NewKnowledgeBase) -> Result<KnowledgeBase> {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        sqlx::query("INSERT INTO knowledge_bases (id, key, name, visibility, owner_user_id, retrieval_config_json, embedding_model_id, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)")
            .bind(&id)
            .bind(&kb.key)
            .bind(&kb.name)
            .bind(&kb.visibility)
            .bind(&kb.owner_user_id)
            .bind(json_string(&kb.retrieval_config))
            .bind(&kb.embedding_model_id)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::KnowledgeBase, e))?;
        self.get_kb(&id).await
    }

    async fn get_kb(&self, id: &str) -> Result<KnowledgeBase> {
        sqlx::query("SELECT * FROM knowledge_bases WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::KnowledgeBase, e))?
            .map(|r| kb_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::KnowledgeBase, id))
    }

    async fn get_kb_by_key(&self, key: &str) -> Result<KnowledgeBase> {
        sqlx::query("SELECT * FROM knowledge_bases WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::KnowledgeBase, e))?
            .map(|r| kb_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::KnowledgeBase, key))
    }

    async fn list_kbs(&self) -> Result<Vec<KnowledgeBase>> {
        let rows = sqlx::query("SELECT * FROM knowledge_bases ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::KnowledgeBase, e))?;
        Ok(rows.iter().map(kb_from_row).collect())
    }

    async fn delete_kb(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM knowledge_bases WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::KnowledgeBase, e))?;
        Ok(())
    }

    async fn update_kb_embedding_model(
        &self,
        id: &str,
        model_id: Option<Id>,
    ) -> Result<KnowledgeBase> {
        sqlx::query(
            "UPDATE knowledge_bases SET embedding_model_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&model_id)
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::KnowledgeBase, e))?;
        self.get_kb(id).await
    }

    async fn create_document(&self, doc: NewDocument) -> Result<Document> {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        sqlx::query("INSERT INTO documents (id, knowledge_base_id, filename, mime_type, size_bytes, file_hash, storage_key, parse_status, index_status, metadata_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'uploaded', 'pending', '{}', ?, ?)")
            .bind(&id)
            .bind(&doc.knowledge_base_id)
            .bind(&doc.filename)
            .bind(&doc.mime_type)
            .bind(doc.size_bytes)
            .bind(&doc.file_hash)
            .bind(&doc.storage_key)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        self.get_document(&id).await
    }

    async fn get_document(&self, id: &str) -> Result<Document> {
        sqlx::query("SELECT * FROM documents WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?
            .map(|r| doc_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Document, id))
    }

    async fn list_documents(&self, kb_id: &str) -> Result<Vec<Document>> {
        let rows = sqlx::query(
            "SELECT * FROM documents WHERE knowledge_base_id = ? ORDER BY created_at DESC",
        )
        .bind(kb_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(rows.iter().map(doc_from_row).collect())
    }

    async fn set_document_status(
        &self,
        id: &str,
        parse_status: &str,
        index_status: &str,
        error: Option<String>,
    ) -> Result<()> {
        sqlx::query("UPDATE documents SET parse_status=?, index_status=?, error_message=?, updated_at=? WHERE id=?")
            .bind(parse_status)
            .bind(index_status)
            .bind(&error)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(())
    }

    async fn delete_document(&self, id: &str) -> Result<()> {
        // chunks 由外键级联删除
        sqlx::query("DELETE FROM documents WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(())
    }

    async fn find_document_by_hash(&self, kb_id: &str, hash: &str) -> Result<Option<Document>> {
        Ok(sqlx::query(
            "SELECT * FROM documents WHERE knowledge_base_id = ? AND file_hash = ? LIMIT 1",
        )
        .bind(kb_id)
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Document, e))?
        .map(|r| doc_from_row(&r)))
    }

    async fn replace_chunks(&self, document_id: &str, chunks: Vec<NewChunk>) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        sqlx::query("DELETE FROM document_chunks WHERE document_id = ?")
            .bind(document_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        tx.commit()
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        for chunk in chunks {
            sqlx::query("INSERT INTO document_chunks (id, document_id, chunk_index, content, content_hash, page_no, section_path, token_count, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, '{}')")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(document_id)
                .bind(chunk.chunk_index as i64)
                .bind(&chunk.content)
                .bind(sha8(&chunk.content))
                .bind(chunk.page_no)
                .bind(&chunk.section_path)
                .bind(chunk.token_count)
                .execute(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Document, e))?;
        }
        Ok(())
    }

    async fn list_chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>> {
        let rows =
            sqlx::query("SELECT * FROM document_chunks WHERE document_id = ? ORDER BY chunk_index")
                .bind(document_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(rows.iter().map(chunk_from_row).collect())
    }

    async fn delete_chunks(&self, document_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM document_chunks WHERE document_id = ?")
            .bind(document_id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(())
    }

    async fn update_chunk_embedding(&self, chunk_id: &str, embedding: Vec<f32>) -> Result<()> {
        sqlx::query("UPDATE document_chunks SET embedding_json = ? WHERE id = ?")
            .bind(serde_json::to_string(&embedding).unwrap_or_default())
            .bind(chunk_id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(())
    }

    async fn count_chunks(&self, kb_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM document_chunks c JOIN documents d ON d.id = c.document_id WHERE d.knowledge_base_id = ?")
            .bind(kb_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Document, e))?;
        Ok(row.get::<i64, _>("c"))
    }
}

fn sha8(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

// ================= Tool / MCP / Agent =================

pub struct SqliteToolRepository {
    pool: SqlitePool,
}

impl SqliteToolRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn tool_from_row(row: &sqlx::sqlite::SqliteRow) -> Tool {
    Tool {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        kind: row.get("kind"),
        input_schema: parse_json(Some(row.get("input_schema_json"))),
        config: parse_json(Some(row.get("config_json"))),
        timeout_ms: row.get("timeout_ms"),
        enabled: row.get::<i64, _>("enabled") != 0,
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

#[async_trait]
impl ToolRepository for SqliteToolRepository {
    async fn create(&self, tool: NewTool) -> Result<Tool> {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        sqlx::query("INSERT INTO tools (id, key, name, description, kind, input_schema_json, config_json, timeout_ms, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)")
            .bind(&id)
            .bind(&tool.key)
            .bind(&tool.name)
            .bind(&tool.description)
            .bind(&tool.kind)
            .bind(json_string(&tool.input_schema))
            .bind(json_string(&tool.config))
            .bind(tool.timeout_ms)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<Tool> {
        sqlx::query("SELECT * FROM tools WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?
            .map(|r| tool_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Tool, id))
    }

    async fn get_by_key(&self, key: &str) -> Result<Tool> {
        sqlx::query("SELECT * FROM tools WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?
            .map(|r| tool_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Tool, key))
    }

    async fn list(&self) -> Result<Vec<Tool>> {
        let rows = sqlx::query("SELECT * FROM tools ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        Ok(rows.iter().map(tool_from_row).collect())
    }

    async fn update(&self, id: &str, update: ToolUpdate) -> Result<Tool> {
        let existing = self.get(id).await?;
        sqlx::query("UPDATE tools SET name=?, description=?, config_json=?, timeout_ms=?, enabled=?, updated_at=? WHERE id=?")
            .bind(update.name.unwrap_or(existing.name))
            .bind(update.description.or(existing.description))
            .bind(json_string(&update.config.unwrap_or(existing.config)))
            .bind(update.timeout_ms.unwrap_or(existing.timeout_ms))
            .bind(update.enabled.unwrap_or(existing.enabled) as i64)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tools WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        Ok(())
    }

    async fn list_enabled(&self) -> Result<Vec<Tool>> {
        let rows = sqlx::query("SELECT * FROM tools WHERE enabled = 1 ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        Ok(rows.iter().map(tool_from_row).collect())
    }
}

pub struct SqliteMcpServerRepository {
    pool: SqlitePool,
}

impl SqliteMcpServerRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn mcp_from_row(row: &sqlx::sqlite::SqliteRow) -> McpServer {
    McpServer {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        transport: row.get("transport"),
        endpoint_or_command: row.get("endpoint_or_command"),
        config: parse_json(Some(row.get("config_json"))),
        enabled: row.get::<i64, _>("enabled") != 0,
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

#[async_trait]
impl McpServerRepository for SqliteMcpServerRepository {
    async fn create(&self, server: NewMcpServer) -> Result<McpServer> {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        sqlx::query("INSERT INTO mcp_servers (id, key, name, transport, endpoint_or_command, config_json, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)")
            .bind(&id)
            .bind(&server.key)
            .bind(&server.name)
            .bind(&server.transport)
            .bind(&server.endpoint_or_command)
            .bind(json_string(&server.config))
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<McpServer> {
        sqlx::query("SELECT * FROM mcp_servers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?
            .map(|r| mcp_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Tool, id))
    }

    async fn get_by_key(&self, key: &str) -> Result<McpServer> {
        sqlx::query("SELECT * FROM mcp_servers WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?
            .map(|r| mcp_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Tool, key))
    }

    async fn list(&self) -> Result<Vec<McpServer>> {
        let rows = sqlx::query("SELECT * FROM mcp_servers ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        Ok(rows.iter().map(mcp_from_row).collect())
    }

    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        sqlx::query("UPDATE mcp_servers SET enabled=?, updated_at=? WHERE id=?")
            .bind(enabled as i64)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Tool, e))?;
        Ok(())
    }
}

pub struct SqliteAgentRepository {
    pool: SqlitePool,
}

impl SqliteAgentRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn agent_from_row(row: &sqlx::sqlite::SqliteRow) -> Agent {
    Agent {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        status: row.get("status"),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn agent_version_from_row(row: &sqlx::sqlite::SqliteRow) -> AgentVersion {
    AgentVersion {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        version: row.get::<i64, _>("version") as i32,
        status: row.get("status"),
        model_ref: row.get("model_ref"),
        prompt_version_id: row.get("prompt_version_id"),
        system_prompt: row.get("system_prompt"),
        max_steps: row.get::<i64, _>("max_steps") as i32,
        max_tool_calls: row.get::<i64, _>("max_tool_calls") as i32,
        timeout_ms: row.get("timeout_ms"),
        max_cost_microunits: row.get("max_cost_microunits"),
        allowed_tools: row
            .get::<Option<String>, _>("allowed_tools_json")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        knowledge_bindings: row
            .get::<Option<String>, _>("knowledge_bindings_json")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
    }
}

fn agent_run_from_row(row: &sqlx::sqlite::SqliteRow) -> AgentRun {
    AgentRun {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        agent_version_id: row.get("agent_version_id"),
        trace_id: row.get("trace_id"),
        status: row.get("status"),
        current_step: row.get::<i64, _>("current_step") as i32,
        max_steps: row.get::<i64, _>("max_steps") as i32,
        started_at: parse_ts(Some(row.get("started_at"))).unwrap_or_default(),
        completed_at: parse_ts(row.get("completed_at")),
        input: parse_json(Some(row.get("metadata_json"))),
        output: row
            .get::<Option<String>, _>("output_ref")
            .map(|s| parse_json(Some(s))),
        error_code: row.get("error_code"),
        error_message: row.get("error_message"),
        cost_microunits: row.get("cost_microunits"),
        metadata: parse_json(Some(row.get("metadata_json"))),
    }
}

fn tool_call_from_row(row: &sqlx::sqlite::SqliteRow) -> ToolCall {
    ToolCall {
        id: row.get("id"),
        trace_id: row.get("trace_id"),
        agent_run_id: row.get("agent_run_id"),
        tool_id: row.get("tool_id"),
        tool_key: row.get("tool_key"),
        status: row.get("status"),
        arguments: parse_json(Some(row.get("arguments_json"))),
        error_code: row.get("error_code"),
        started_at: parse_ts(Some(row.get("started_at"))).unwrap_or_default(),
        completed_at: parse_ts(row.get("completed_at")),
        latency_ms: row.get("latency_ms"),
    }
}

#[async_trait]
impl AgentRepository for SqliteAgentRepository {
    async fn create(
        &self,
        agent: NewAgent,
        version: NewAgentVersion,
    ) -> Result<(Agent, AgentVersion)> {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        sqlx::query("INSERT INTO agents (id, key, name, description, status, created_at, updated_at) VALUES (?, ?, ?, ?, 'active', ?, ?)")
            .bind(&id)
            .bind(&agent.key)
            .bind(&agent.name)
            .bind(&agent.description)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        let v = self
            .create_version(NewAgentVersion {
                agent_id: id.clone(),
                ..version
            })
            .await?;
        Ok((self.get(&id).await?, v))
    }

    async fn get(&self, id: &str) -> Result<Agent> {
        sqlx::query("SELECT * FROM agents WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?
            .map(|r| agent_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Agent, id))
    }

    async fn get_by_key(&self, key: &str) -> Result<Agent> {
        sqlx::query("SELECT * FROM agents WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?
            .map(|r| agent_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Agent, key))
    }

    async fn list(&self) -> Result<Vec<Agent>> {
        let rows = sqlx::query("SELECT * FROM agents ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(rows.iter().map(agent_from_row).collect())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM agents WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(())
    }

    async fn create_version(&self, version: NewAgentVersion) -> Result<AgentVersion> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(version), 0) AS v FROM agent_versions WHERE agent_id = ?",
        )
        .bind(&version.agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Agent, e))?;
        let next = row.get::<i64, _>("v") + 1;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO agent_versions (id, agent_id, version, status, model_ref, prompt_version_id, system_prompt, max_steps, max_tool_calls, timeout_ms, max_cost_microunits, allowed_tools_json, knowledge_bindings_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&version.agent_id)
            .bind(next)
            .bind(&version.status)
            .bind(&version.model_ref)
            .bind(&version.prompt_version_id)
            .bind(&version.system_prompt)
            .bind(version.max_steps as i64)
            .bind(version.max_tool_calls as i64)
            .bind(version.timeout_ms)
            .bind(version.max_cost_microunits)
            .bind(json_string(&serde_json::to_value(&version.allowed_tools).unwrap_or_default()))
            .bind(json_string(&serde_json::to_value(&version.knowledge_bindings).unwrap_or_default()))
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        self.get_version(&id).await
    }

    async fn versions_for(&self, agent_id: &str) -> Result<Vec<AgentVersion>> {
        let rows =
            sqlx::query("SELECT * FROM agent_versions WHERE agent_id = ? ORDER BY version DESC")
                .bind(agent_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(rows.iter().map(agent_version_from_row).collect())
    }

    async fn get_version(&self, version_id: &str) -> Result<AgentVersion> {
        sqlx::query("SELECT * FROM agent_versions WHERE id = ?")
            .bind(version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?
            .map(|r| agent_version_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Agent, version_id))
    }

    async fn published_version(&self, agent_id: &str) -> Result<Option<AgentVersion>> {
        Ok(sqlx::query("SELECT * FROM agent_versions WHERE agent_id = ? AND status = 'published' ORDER BY version DESC LIMIT 1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?
            .map(|r| agent_version_from_row(&r)))
    }

    async fn set_version_status(&self, version_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE agent_versions SET status = ? WHERE id = ?")
            .bind(status)
            .bind(version_id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(())
    }

    async fn create_run(&self, run: NewAgentRun) -> Result<AgentRun> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO agent_runs (id, agent_id, agent_version_id, trace_id, status, current_step, max_steps, started_at, input_ref, usage_json, metadata_json) VALUES (?, ?, ?, ?, 'running', 0, ?, ?, ?, '{}', ?)")
            .bind(&id)
            .bind(&run.agent_id)
            .bind(&run.agent_version_id)
            .bind(&run.trace_id)
            .bind(run.max_steps as i64)
            .bind(now())
            .bind(run.input.to_string())
            .bind(json_string(&run.metadata))
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        self.get_run(&id).await
    }

    async fn finish_run(
        &self,
        id: &str,
        status: &str,
        output: Option<Value>,
        error_code: Option<String>,
        error_message: Option<String>,
        cost: i64,
    ) -> Result<()> {
        sqlx::query("UPDATE agent_runs SET status=?, completed_at=?, output_ref=?, error_code=?, error_message=?, cost_microunits=? WHERE id=?")
            .bind(status)
            .bind(now())
            .bind(output.map(|v| v.to_string()))
            .bind(&error_code)
            .bind(&error_message)
            .bind(cost)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(())
    }

    async fn get_run(&self, id: &str) -> Result<AgentRun> {
        sqlx::query("SELECT * FROM agent_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?
            .map(|r| agent_run_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Agent, id))
    }

    async fn list_runs(&self, agent_id: &str, limit: u64) -> Result<Vec<AgentRun>> {
        let rows = sqlx::query(
            "SELECT * FROM agent_runs WHERE agent_id = ? ORDER BY started_at DESC LIMIT ?",
        )
        .bind(agent_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(rows.iter().map(agent_run_from_row).collect())
    }

    async fn insert_tool_call(&self, call: ToolCall) -> Result<()> {
        sqlx::query("INSERT INTO tool_calls (id, trace_id, agent_run_id, tool_id, tool_key, status, arguments_json, error_code, started_at, completed_at, latency_ms, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '{}')")
            .bind(&call.id)
            .bind(&call.trace_id)
            .bind(&call.agent_run_id)
            .bind(&call.tool_id)
            .bind(&call.tool_key)
            .bind(&call.status)
            .bind(json_string(&call.arguments))
            .bind(&call.error_code)
            .bind(now())
            .bind(call.completed_at.map(|_| now()))
            .bind(call.latency_ms)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(())
    }

    async fn tool_calls_for_run(&self, run_id: &str) -> Result<Vec<ToolCall>> {
        let rows =
            sqlx::query("SELECT * FROM tool_calls WHERE agent_run_id = ? ORDER BY started_at")
                .bind(run_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Agent, e))?;
        Ok(rows.iter().map(tool_call_from_row).collect())
    }
}

// ================= Eval =================

pub struct SqliteEvalRepository {
    pool: SqlitePool,
}

impl SqliteEvalRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn dataset_from_row(row: &sqlx::sqlite::SqliteRow) -> EvalDataset {
    EvalDataset {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
    }
}

fn case_from_row(row: &sqlx::sqlite::SqliteRow) -> EvalCase {
    EvalCase {
        id: row.get("id"),
        dataset_id: row.get("dataset_id"),
        name: row.get("name"),
        input: parse_json(Some(row.get("input_json"))),
        expected_output: row.get("expected_output"),
        metadata: parse_json(Some(row.get("metadata_json"))),
    }
}

fn eval_run_from_row(row: &sqlx::sqlite::SqliteRow) -> EvalRun {
    EvalRun {
        id: row.get("id"),
        dataset_id: row.get("dataset_id"),
        label: row.get("label"),
        candidate: parse_json(Some(row.get("candidate_json"))),
        judge_config: parse_json(Some(row.get("judge_config_json"))),
        status: row.get("status"),
        started_at: parse_ts(Some(row.get("started_at"))).unwrap_or_default(),
        completed_at: parse_ts(row.get("completed_at")),
        summary: parse_json(Some(row.get("summary_json"))),
        error: row.get("error"),
    }
}

fn eval_result_from_row(row: &sqlx::sqlite::SqliteRow) -> EvalResult {
    EvalResult {
        id: row.get("id"),
        run_id: row.get("run_id"),
        case_id: row.get("case_id"),
        response_text: row.get("response_text"),
        latency_ms: row.get("latency_ms"),
        input_tokens: row.get("input_tokens"),
        output_tokens: row.get("output_tokens"),
        cost_microunits: row.get("cost_microunits"),
        score: parse_json(Some(row.get("score_json"))),
        judge: row
            .get::<Option<String>, _>("judge_json")
            .map(|s| parse_json(Some(s))),
    }
}

#[async_trait]
impl EvalRepository for SqliteEvalRepository {
    async fn create_dataset(
        &self,
        key: &str,
        name: &str,
        description: Option<String>,
    ) -> Result<EvalDataset> {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now();
        sqlx::query("INSERT INTO eval_datasets (id, key, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(key)
            .bind(name)
            .bind(&description)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        sqlx::query("SELECT * FROM eval_datasets WHERE id = ?")
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map(|r| dataset_from_row(&r))
            .map_err(|e| db_error(DomainResource::Eval, e))
    }

    async fn get_dataset(&self, id: &str) -> Result<EvalDataset> {
        sqlx::query("SELECT * FROM eval_datasets WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?
            .map(|r| dataset_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Eval, id))
    }

    async fn list_datasets(&self) -> Result<Vec<EvalDataset>> {
        let rows = sqlx::query("SELECT * FROM eval_datasets ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(rows.iter().map(dataset_from_row).collect())
    }

    async fn delete_dataset(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM eval_datasets WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(())
    }

    async fn add_case(
        &self,
        dataset_id: &str,
        name: &str,
        input: Value,
        expected: Option<String>,
    ) -> Result<EvalCase> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO eval_cases (id, dataset_id, name, input_json, expected_output, metadata_json, created_at) VALUES (?, ?, ?, ?, ?, '{}', ?)")
            .bind(&id)
            .bind(dataset_id)
            .bind(name)
            .bind(input.to_string())
            .bind(&expected)
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(EvalCase {
            id,
            dataset_id: dataset_id.to_string(),
            name: name.to_string(),
            input,
            expected_output: expected,
            metadata: serde_json::json!({}),
        })
    }

    async fn list_cases(&self, dataset_id: &str) -> Result<Vec<EvalCase>> {
        let rows = sqlx::query("SELECT * FROM eval_cases WHERE dataset_id = ? ORDER BY created_at")
            .bind(dataset_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(rows.iter().map(case_from_row).collect())
    }

    async fn delete_case(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM eval_cases WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(())
    }

    async fn create_run(
        &self,
        dataset_id: &str,
        label: &str,
        candidate: Value,
        judge_config: Value,
    ) -> Result<EvalRun> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO eval_runs (id, dataset_id, label, candidate_json, judge_config_json, status, started_at, summary_json) VALUES (?, ?, ?, ?, ?, 'running', ?, '{}')")
            .bind(&id)
            .bind(dataset_id)
            .bind(label)
            .bind(candidate.to_string())
            .bind(judge_config.to_string())
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        sqlx::query("SELECT * FROM eval_runs WHERE id = ?")
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map(|r| eval_run_from_row(&r))
            .map_err(|e| db_error(DomainResource::Eval, e))
    }

    async fn finish_run(
        &self,
        id: &str,
        status: &str,
        summary: Value,
        error: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE eval_runs SET status=?, completed_at=?, summary_json=?, error=? WHERE id=?",
        )
        .bind(status)
        .bind(now())
        .bind(summary.to_string())
        .bind(&error)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(())
    }

    async fn get_run(&self, id: &str) -> Result<EvalRun> {
        sqlx::query("SELECT * FROM eval_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?
            .map(|r| eval_run_from_row(&r))
            .ok_or_else(|| DomainError::not_found(DomainResource::Eval, id))
    }

    async fn list_runs(&self, dataset_id: &str) -> Result<Vec<EvalRun>> {
        let rows =
            sqlx::query("SELECT * FROM eval_runs WHERE dataset_id = ? ORDER BY started_at DESC")
                .bind(dataset_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(rows.iter().map(eval_run_from_row).collect())
    }

    async fn insert_result(&self, result: EvalResult) -> Result<()> {
        sqlx::query("INSERT INTO eval_results (id, run_id, case_id, response_text, latency_ms, input_tokens, output_tokens, cost_microunits, score_json, judge_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&result.id)
            .bind(&result.run_id)
            .bind(&result.case_id)
            .bind(&result.response_text)
            .bind(result.latency_ms)
            .bind(result.input_tokens)
            .bind(result.output_tokens)
            .bind(result.cost_microunits)
            .bind(json_string(&result.score))
            .bind(result.judge.map(|j| j.to_string()))
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(())
    }

    async fn results_for_run(&self, run_id: &str) -> Result<Vec<EvalResult>> {
        let rows = sqlx::query("SELECT * FROM eval_results WHERE run_id = ? ORDER BY created_at")
            .bind(run_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Eval, e))?;
        Ok(rows.iter().map(eval_result_from_row).collect())
    }
}

// ================= Policy =================

pub struct SqlitePolicyRepository {
    pool: SqlitePool,
}

impl SqlitePolicyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PolicyRepository for SqlitePolicyRepository {
    async fn upsert_routing_policy(&self, policy: &RoutingPolicy) -> Result<()> {
        sqlx::query("INSERT INTO routing_policies (id, key, name, priority, match_json, action_json, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(key) DO UPDATE SET name=excluded.name, priority=excluded.priority, match_json=excluded.match_json, action_json=excluded.action_json, enabled=excluded.enabled, updated_at=excluded.updated_at")
            .bind(if policy.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { policy.id.clone() })
            .bind(&policy.key)
            .bind(&policy.name)
            .bind(policy.priority as i64)
            .bind(json_string(&policy.match_rules))
            .bind(json_string(&policy.action))
            .bind(policy.enabled as i64)
            .bind(now())
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Policy, e))?;
        Ok(())
    }

    async fn list_routing_policies(&self) -> Result<Vec<RoutingPolicy>> {
        let rows = sqlx::query("SELECT * FROM routing_policies ORDER BY priority")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Policy, e))?;
        Ok(rows
            .iter()
            .map(|row| RoutingPolicy {
                id: row.get("id"),
                key: row.get("key"),
                name: row.get("name"),
                priority: row.get::<i64, _>("priority") as i32,
                match_rules: parse_json(Some(row.get("match_json"))),
                action: parse_json(Some(row.get("action_json"))),
                enabled: row.get::<i64, _>("enabled") != 0,
            })
            .collect())
    }

    async fn delete_routing_policy(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM routing_policies WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Policy, e))?;
        Ok(())
    }

    async fn upsert_security_policy(&self, policy: &SecurityPolicy) -> Result<()> {
        sqlx::query("INSERT INTO security_policies (id, key, name, policy_type, priority, rule_json, action_json, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(key) DO UPDATE SET name=excluded.name, policy_type=excluded.policy_type, priority=excluded.priority, rule_json=excluded.rule_json, action_json=excluded.action_json, enabled=excluded.enabled, updated_at=excluded.updated_at")
            .bind(if policy.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { policy.id.clone() })
            .bind(&policy.key)
            .bind(&policy.name)
            .bind(&policy.policy_type)
            .bind(policy.priority as i64)
            .bind(json_string(&policy.rule))
            .bind(json_string(&policy.action))
            .bind(policy.enabled as i64)
            .bind(now())
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Policy, e))?;
        Ok(())
    }

    async fn list_security_policies(
        &self,
        policy_type: Option<&str>,
    ) -> Result<Vec<SecurityPolicy>> {
        let sql = match policy_type {
            Some(t) => format!(
                "SELECT * FROM security_policies WHERE policy_type = '{t}' ORDER BY priority"
            ),
            None => "SELECT * FROM security_policies ORDER BY priority".to_string(),
        };
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Policy, e))?;
        Ok(rows
            .iter()
            .map(|row| SecurityPolicy {
                id: row.get("id"),
                key: row.get("key"),
                name: row.get("name"),
                policy_type: row.get("policy_type"),
                priority: row.get::<i64, _>("priority") as i32,
                rule: parse_json(Some(row.get("rule_json"))),
                action: parse_json(Some(row.get("action_json"))),
                enabled: row.get::<i64, _>("enabled") != 0,
            })
            .collect())
    }

    async fn delete_security_policy(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM security_policies WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Policy, e))?;
        Ok(())
    }
}
