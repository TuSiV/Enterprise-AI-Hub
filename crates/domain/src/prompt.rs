//! Prompt 聚合（方案 §9.13 / §18.1 / §39.3）：
//! Published 版本不可原地修改，修改必须创建新版本。

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;

pub type Id = String;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Prompt {
    pub id: Id,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<Id>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptVersionStatus {
    Draft,
    Testing,
    Published,
    Deprecated,
}

impl PromptVersionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PromptVersionStatus::Draft => "draft",
            PromptVersionStatus::Testing => "testing",
            PromptVersionStatus::Published => "published",
            PromptVersionStatus::Deprecated => "deprecated",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(PromptVersionStatus::Draft),
            "testing" => Some(PromptVersionStatus::Testing),
            "published" => Some(PromptVersionStatus::Published),
            "deprecated" => Some(PromptVersionStatus::Deprecated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptVersion {
    pub id: Id,
    pub prompt_id: Id,
    pub version: i32,
    pub status: PromptVersionStatus,
    pub system_template: Option<String>,
    pub user_template: Option<String>,
    pub variables_schema: serde_json::Value,
    pub model_config: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub created_by: Option<Id>,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait PromptRepository: Send + Sync {
    async fn create(&self, prompt: NewPrompt) -> Result<Prompt, DomainError>;
    async fn get(&self, id: &str) -> Result<Prompt, DomainError>;
    async fn get_by_key(&self, key: &str) -> Result<Prompt, DomainError>;
    async fn list(&self) -> Result<Vec<Prompt>, DomainError>;
    async fn update(
        &self,
        id: &str,
        name: String,
        description: Option<String>,
    ) -> Result<Prompt, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;

    async fn create_version(&self, version: NewPromptVersion)
        -> Result<PromptVersion, DomainError>;
    async fn versions_for(&self, prompt_id: &str) -> Result<Vec<PromptVersion>, DomainError>;
    async fn get_version(&self, version_id: &str) -> Result<PromptVersion, DomainError>;
    async fn published_version(
        &self,
        prompt_id: &str,
    ) -> Result<Option<PromptVersion>, DomainError>;
    /// 状态流转（§39.3）；Published → 其它状态视为废弃性操作由调用方约束。
    async fn set_version_status(
        &self,
        version_id: &str,
        status: PromptVersionStatus,
    ) -> Result<PromptVersion, DomainError>;
}

#[derive(Debug, Clone)]
pub struct NewPrompt {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<Id>,
}

#[derive(Debug, Clone)]
pub struct NewPromptVersion {
    pub prompt_id: Id,
    pub system_template: Option<String>,
    pub user_template: Option<String>,
    pub variables_schema: serde_json::Value,
    pub model_config: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub created_by: Option<Id>,
}
