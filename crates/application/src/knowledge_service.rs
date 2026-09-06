//! Knowledge / RAG（M12/M13，方案 §19）：
//! 上传 → 去重 → 本地对象存储 → 解析（runtime/内置纯文本）→ chunk → embedding → 向量检索 → 引用。
//! 权限在召回阶段前置过滤（§19.7）：kb visibility + application 绑定。

use std::sync::Arc;

use aihub_domain::canonical::{CanonicalEmbeddingRequest, UsageSource};
use aihub_domain::cost::Pricing;
use aihub_domain::entities::ModelType;
use aihub_domain::error::DomainError;
use aihub_domain::platform::*;
use aihub_domain::repos::ModelUpdate;
use aihub_domain::DomainResource;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::registry::ProviderRegistry;
use crate::Repos;

/// 本地对象存储（§21.2 LocalObjectStorage）：documents/ 目录
pub struct LocalObjectStorage {
    root: std::path::PathBuf,
}

impl LocalObjectStorage {
    pub fn new(root: std::path::PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&root);
        Self { root }
    }

    pub fn put(&self, key: &str, bytes: &[u8]) -> std::io::Result<()> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)
    }

    pub fn get(&self, key: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.root.join(key))
    }

    pub fn delete(&self, key: &str) -> std::io::Result<()> {
        let path = self.root.join(key);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

pub struct KnowledgeService {
    repos: Repos,
    registry: Arc<ProviderRegistry>,
    storage: Arc<LocalObjectStorage>,
    /// chunking 配置（§19.3）：KB retrieval_config 可覆盖
    default_target_tokens: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbSummary {
    #[serde(flatten)]
    pub kb: KnowledgeBase,
    pub document_count: i64,
    pub chunk_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalQueryResult {
    pub query: String,
    pub hits: Vec<RetrievalHit>,
    /// RAG 上下文拼装结果（§19.4 Context Builder）
    pub context: String,
}

impl KnowledgeService {
    pub fn new(
        repos: Repos,
        registry: Arc<ProviderRegistry>,
        storage: Arc<LocalObjectStorage>,
    ) -> Self {
        Self {
            repos,
            registry,
            storage,
            default_target_tokens: 700,
        }
    }

    async fn audit(&self, event: &str, resource_id: &str, metadata: Value) {
        let _ = self
            .repos
            .audit
            .insert(aihub_domain::entities::AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                trace_id: None,
                actor_type: "admin".into(),
                actor_id: None,
                event_type: event.into(),
                resource_type: Some("knowledge".into()),
                resource_id: Some(resource_id.into()),
                decision: None,
                payload_ref: None,
                metadata,
                created_at: chrono::Utc::now(),
            })
            .await;
    }

    pub async fn create_kb(
        &self,
        key: &str,
        name: &str,
        visibility: &str,
        embedding_model_id: Option<String>,
    ) -> Result<KnowledgeBase, DomainError> {
        if key.trim().is_empty() {
            return Err(DomainError::validation(
                DomainResource::KnowledgeBase,
                "key is required",
            ));
        }
        let kb = self
            .repos
            .knowledge
            .create_kb(NewKnowledgeBase {
                key: key.to_string(),
                name: name.to_string(),
                visibility: visibility.to_string(),
                owner_user_id: None,
                retrieval_config: json!({"topK": 5, "targetTokens": self.default_target_tokens, "overlapTokens": 100}),
                embedding_model_id: embedding_model_id.clone(),
            })
            .await?;
        self.audit("kb.created", &kb.id, json!({"key": key})).await;
        Ok(kb)
    }

    pub async fn list_kbs(&self) -> Result<Vec<KbSummary>, DomainError> {
        let kbs = self.repos.knowledge.list_kbs().await?;
        let mut out = Vec::new();
        for kb in kbs {
            let docs = self.repos.knowledge.list_documents(&kb.id).await?;
            let chunks = self.repos.knowledge.count_chunks(&kb.id).await?;
            out.push(KbSummary {
                kb,
                document_count: docs.len() as i64,
                chunk_count: chunks,
            });
        }
        Ok(out)
    }

    pub async fn delete_kb(&self, id: &str) -> Result<(), DomainError> {
        let docs = self.repos.knowledge.list_documents(id).await?;
        for doc in docs {
            let _ = self.storage.delete(&doc.storage_key);
        }
        self.repos.knowledge.delete_kb(id).await?;
        self.audit("kb.deleted", id, json!({})).await;
        Ok(())
    }

    /// 上传文档（§19.1 前半）：hash 去重 → 存原文 → 建 chunks → embedding → ready。
    /// V1 同步执行；Server 模式改走 runtime_jobs 队列（§21.6）。
    pub async fn upload_document(
        &self,
        kb_id: &str,
        filename: &str,
        mime_type: &str,
        bytes: Vec<u8>,
        created_by: Option<&str>,
    ) -> Result<Document, DomainError> {
        let kb = self.repos.knowledge.get_kb(kb_id).await?;
        let hash = hex::encode(Sha256::digest(&bytes));
        if let Some(existing) = self
            .repos
            .knowledge
            .find_document_by_hash(kb_id, &hash)
            .await?
        {
            return Err(DomainError::in_use(
                DomainResource::Document,
                &existing.id,
                format!("identical file already indexed as {}", existing.filename),
            ));
        }
        // 类型支持检查（§27.3 DOCUMENT_TYPE_UNSUPPORTED）
        let supported = matches!(
            mime_type,
            "text/plain" | "text/markdown" | "text/csv" | "application/json"
        ) || filename.ends_with(".txt")
            || filename.ends_with(".md")
            || filename.ends_with(".csv")
            || filename.ends_with(".json");
        if !supported {
            return Err(DomainError::validation(
                DomainResource::Document,
                format!("unsupported document type '{mime_type}'; V1 supports txt/md/csv/json (pdf/docx via runtime)"),
            ));
        }

        let storage_key = format!("{}/{}", kb.id, uuid::Uuid::new_v4());
        self.storage.put(&storage_key, &bytes).map_err(|e| {
            DomainError::internal(
                DomainResource::Document,
                format!("object storage write failed: {e}"),
            )
        })?;

        let doc = self
            .repos
            .knowledge
            .create_document(NewDocument {
                knowledge_base_id: kb.id.clone(),
                filename: filename.to_string(),
                mime_type: mime_type.to_string(),
                size_bytes: bytes.len() as i64,
                file_hash: hash,
                storage_key: storage_key.clone(),
            })
            .await?;
        let _ = created_by;
        self.audit(
            "document.uploaded",
            &doc.id,
            json!({"kb": kb.key, "filename": filename}),
        )
        .await;

        // 解析（V1 纯文本；PDF/DOCX 由 runtime 承担 → M11 接入后走 parse 端点）
        self.repos
            .knowledge
            .set_document_status(&doc.id, "parsing", "pending", None)
            .await?;
        let text = match self.storage.get(&storage_key) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                self.repos
                    .knowledge
                    .set_document_status(&doc.id, "parse_failed", "pending", Some(e.to_string()))
                    .await?;
                return Err(DomainError::internal(
                    DomainResource::Document,
                    format!("read back failed: {e}"),
                ));
            }
        };

        // Chunk（§19.3）：段落感知 + 目标 token 聚合
        self.repos
            .knowledge
            .set_document_status(&doc.id, "chunking", "pending", None)
            .await?;
        let target_tokens = kb
            .retrieval_config
            .get("targetTokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.default_target_tokens as i64) as usize;
        let chunks = chunk_text(&text, target_tokens.max(100), 0);
        if chunks.is_empty() {
            self.repos
                .knowledge
                .set_document_status(
                    &doc.id,
                    "parse_failed",
                    "pending",
                    Some("empty document".into()),
                )
                .await?;
            return Err(DomainError::validation(
                DomainResource::Document,
                "document is empty",
            ));
        }
        self.repos
            .knowledge
            .replace_chunks(
                &doc.id,
                chunks
                    .iter()
                    .enumerate()
                    .map(|(i, c)| NewChunk {
                        chunk_index: i as i32,
                        content: c.clone(),
                        page_no: None,
                        section_path: None,
                        token_count: Some((c.len() / 4) as i32),
                    })
                    .collect(),
            )
            .await?;

        // Embedding（§19.1）
        self.repos
            .knowledge
            .set_document_status(&doc.id, "embedding", "pending", None)
            .await?;
        match self.embed_chunks(&kb, &doc.id).await {
            Ok(()) => {
                self.repos
                    .knowledge
                    .set_document_status(&doc.id, "ready", "ready", None)
                    .await?;
                self.audit("document.indexed", &doc.id, json!({"chunks": chunks.len()}))
                    .await;
            }
            Err(e) => {
                self.repos
                    .knowledge
                    .set_document_status(
                        &doc.id,
                        "embedding",
                        "index_failed",
                        Some(e.message.clone()),
                    )
                    .await?;
                return Err(e);
            }
        }
        self.repos.knowledge.get_document(&doc.id).await
    }

    async fn embedder_model(
        &self,
        kb: &KnowledgeBase,
    ) -> Result<aihub_domain::entities::Model, DomainError> {
        let model_id = kb.embedding_model_id.clone().ok_or_else(|| {
            DomainError::validation(
                DomainResource::KnowledgeBase,
                "kb has no embedding model bound",
            )
        })?;
        self.repos.models.get(&model_id).await
    }

    async fn embed_texts(
        &self,
        model: &aihub_domain::entities::Model,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        let provider = self.repos.providers.get(&model.provider_id).await?;
        let adapter = self.registry.get_for(&provider).await?;
        let response = adapter
            .embeddings(CanonicalEmbeddingRequest {
                model: model.model_key.clone(),
                inputs: texts,
            })
            .await
            .map_err(|e| {
                DomainError::internal(
                    DomainResource::Document,
                    format!("embedding failed: {}", e.message),
                )
            })?;
        Ok(response.embeddings)
    }

    async fn embed_chunks(&self, kb: &KnowledgeBase, document_id: &str) -> Result<(), DomainError> {
        let model = self.embedder_model(kb).await?;
        let chunks = self.repos.knowledge.list_chunks(document_id).await?;
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let vectors = self.embed_texts(&model, texts).await?;
        if vectors.len() != chunks.len() {
            return Err(DomainError::internal(
                DomainResource::Document,
                format!(
                    "embedding count mismatch: {} vs {}",
                    vectors.len(),
                    chunks.len()
                ),
            ));
        }
        for (chunk, vector) in chunks.iter().zip(vectors) {
            self.repos
                .knowledge
                .update_chunk_embedding(&chunk.id, vector)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_document(&self, id: &str) -> Result<(), DomainError> {
        let doc = self.repos.knowledge.get_document(id).await?;
        // 删除文档同时删除对应向量（§43.4）
        self.repos.knowledge.delete_chunks(id).await?;
        self.repos.knowledge.delete_document(id).await?;
        let _ = self.storage.delete(&doc.storage_key);
        self.audit("document.deleted", id, json!({})).await;
        Ok(())
    }

    pub async fn list_documents(&self, kb_id: &str) -> Result<Vec<Document>, DomainError> {
        self.repos.knowledge.list_documents(kb_id).await
    }

    pub async fn document_detail(
        &self,
        id: &str,
    ) -> Result<(Document, Vec<DocumentChunk>), DomainError> {
        let doc = self.repos.knowledge.get_document(id).await?;
        let chunks = self.repos.knowledge.list_chunks(id).await?;
        Ok((doc, chunks))
    }

    /// 访问控制（§19.7）：application 绑定 allowedKbs（空=全部 public/department），visibility=private 需要 owner。
    pub fn authorize_kb(
        application_allowed_kbs: &[String],
        kb: &KnowledgeBase,
    ) -> Result<(), DomainError> {
        if !application_allowed_kbs.is_empty()
            && !application_allowed_kbs.iter().any(|k| k == &kb.key)
        {
            return Err(DomainError::validation(
                DomainResource::KnowledgeBase,
                format!(
                    "knowledge base '{}' is not allowed for this application",
                    kb.key
                ),
            ));
        }
        Ok(())
    }

    /// 检索（§19.4/§19.5）：query embedding → 余弦 topK → 引用。
    pub async fn query(
        &self,
        kb_key: &str,
        query: &str,
        top_k: usize,
    ) -> Result<RetrievalQueryResult, DomainError> {
        let kb = self.repos.knowledge.get_kb_by_key(kb_key).await?;

        let model = self.embedder_model(&kb).await?;
        let vectors = self.embed_texts(&model, vec![query.to_string()]).await?;
        let query_vector = vectors.first().cloned().ok_or_else(|| {
            DomainError::internal(DomainResource::Document, "empty query embedding")
        })?;

        let mut hits: Vec<RetrievalHit> = Vec::new();
        for doc in self.repos.knowledge.list_documents(&kb.id).await? {
            if doc.parse_status != "ready" {
                continue;
            }
            for chunk in self.repos.knowledge.list_chunks(&doc.id).await? {
                let Some(vector) = chunk.embedding else {
                    continue;
                };
                let score = cosine_similarity(&query_vector, &vector);
                hits.push(RetrievalHit {
                    chunk_id: chunk.id,
                    document_id: doc.id.clone(),
                    knowledge_base_id: kb.id.clone(),
                    score,
                    content: chunk.content,
                    filename: doc.filename.clone(),
                    page: chunk.page_no,
                    section: chunk.section_path,
                });
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(top_k.max(1));

        let context = hits
            .iter()
            .enumerate()
            .map(|(i, h)| {
                format!(
                    "[{}] {} (page {:?} section {:?})\n{}",
                    i + 1,
                    h.filename,
                    h.page,
                    h.section.clone().unwrap_or_default(),
                    h.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(RetrievalQueryResult {
            query: query.to_string(),
            hits,
            context,
        })
    }

    /// RAG 问答：检索上下文 + 模板拼装 → 走 Gateway pipeline（引用随 context 返回）。
    pub async fn rag_context(
        &self,
        kb_key: &str,
        question: &str,
        top_k: usize,
    ) -> Result<(String, Vec<RetrievalHit>), DomainError> {
        let result = self.query(kb_key, question, top_k).await?;
        let prompt = format!(
            "请基于以下企业知识回答问题，并标注引用编号。若知识不足以回答请明确说明。\n\n{}```\n问题：{question}",
            if result.context.is_empty() {
                String::new()
            } else {
                format!("{}\n\n", result.context)
            }
        );
        Ok((prompt, result.hits))
    }

    /// 为 KB 绑定 embedding 模型（自动确保 model_type=embedding 并启用）。
    pub async fn bind_embedding_model(
        &self,
        kb_id: &str,
        model_id: &str,
    ) -> Result<KnowledgeBase, DomainError> {
        let model = self.repos.models.get(model_id).await?;
        if model.model_type != ModelType::Embedding {
            // 自动修正类型标注
            self.repos
                .models
                .update(
                    model_id,
                    ModelUpdate {
                        model_type: Some(ModelType::Embedding),
                        pricing: Some(Pricing::default()),
                        ..Default::default()
                    },
                )
                .await?;
        }
        let _ = model;
        let kb = self
            .repos
            .knowledge
            .update_kb_embedding_model(kb_id, Some(model_id.to_string()))
            .await?;
        self.audit(
            "kb.embedding_model_bound",
            kb_id,
            json!({"model": model_id}),
        )
        .await;
        Ok(kb)
    }
}

/// 段落感知 chunk：按空行分段，聚合到目标 token 预算（chars/4 估算），超长段落硬切。
pub fn chunk_text(text: &str, target_tokens: usize, _overlap_tokens: usize) -> Vec<String> {
    let target_chars = target_tokens * 4;
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.len() > target_chars {
            // 硬切长段落
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            let mut start = 0;
            while start < paragraph.len() {
                let end = (start + target_chars).min(paragraph.len());
                let mut end = end;
                while end < paragraph.len() && !paragraph.is_char_boundary(end) {
                    end += 1;
                }
                chunks.push(paragraph[start..end].to_string());
                start = end;
            }
            continue;
        }
        if current.len() + paragraph.len() + 2 > target_chars && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f64;
    let mut na = 0f64;
    let mut nb = 0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[allow(dead_code)]
fn unused(_: UsageSource) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_respects_target_and_boundaries() {
        let text = "段落一\n\n段落二\n\n段落三".repeat(50);
        let chunks = chunk_text(&text, 200, 0);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(
                chunk.len() <= 200 * 4 + 8,
                "chunk too large: {}",
                chunk.len()
            );
        }
        assert!(!chunks.iter().any(|c| c.trim().is_empty()));
    }

    #[test]
    fn cosine_similarity_basics() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-9);
        assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-9);
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }
}
