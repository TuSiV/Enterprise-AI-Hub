//! ObjectStorage Port（方案 §21.2）：本地文件系统 / S3-compatible。

use async_trait::async_trait;
use serde_json::Value;

use crate::error::DomainError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size_bytes: i64,
    #[serde(flatten)]
    pub extra: Value,
}

#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn put(&self, key: &str, body: Vec<u8>) -> Result<ObjectMeta, DomainError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, DomainError>;
    async fn delete(&self, key: &str) -> Result<(), DomainError>;
    async fn exists(&self, key: &str) -> Result<bool, DomainError>;
}
