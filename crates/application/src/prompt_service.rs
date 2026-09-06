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

//! Prompt Center（方案 §18）：版本生命周期 draft→testing→published→deprecated；
//! Published 版本不可原地修改（新版本发布时旧 published 自动 deprecated，保证唯一可追溯）。

use aihub_domain::error::DomainError;
use aihub_domain::prompt::*;
use aihub_domain::DomainResource;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::Repos;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDetail {
    #[serde(flatten)]
    pub prompt: Prompt,
    pub versions: Vec<PromptVersion>,
    pub published_version: Option<i32>,
}

pub struct PromptService {
    repos: Repos,
}

impl PromptService {
    pub fn new(repos: Repos) -> Self {
        Self { repos }
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
                resource_type: Some("prompt".into()),
                resource_id: Some(resource_id.into()),
                decision: None,
                payload_ref: None,
                metadata,
                created_at: chrono::Utc::now(),
            })
            .await;
    }

    pub async fn create(
        &self,
        key: String,
        name: String,
        description: Option<String>,
    ) -> Result<Prompt, DomainError> {
        if key.trim().is_empty() {
            return Err(DomainError::validation(
                DomainResource::Prompt,
                "key is required",
            ));
        }
        let prompt = self
            .repos
            .prompts
            .create(NewPrompt {
                key: key.clone(),
                name,
                description,
                owner_id: None,
            })
            .await?;
        self.audit("prompt.created", &prompt.id, json!({"key": key}))
            .await;
        Ok(prompt)
    }

    pub async fn detail(&self, id_or_key: &str) -> Result<PromptDetail, DomainError> {
        let prompt = match self.repos.prompts.get(id_or_key).await {
            Ok(p) => p,
            Err(_) => self.repos.prompts.get_by_key(id_or_key).await?,
        };
        let versions = self.repos.prompts.versions_for(&prompt.id).await?;
        let published = self.repos.prompts.published_version(&prompt.id).await?;
        Ok(PromptDetail {
            prompt,
            versions,
            published_version: published.map(|v| v.version),
        })
    }

    pub async fn list(&self) -> Result<Vec<PromptDetail>, DomainError> {
        let prompts = self.repos.prompts.list().await?;
        let mut details = Vec::new();
        for prompt in prompts {
            let versions = self.repos.prompts.versions_for(&prompt.id).await?;
            let published = self.repos.prompts.published_version(&prompt.id).await?;
            details.push(PromptDetail {
                prompt,
                versions,
                published_version: published.map(|v| v.version),
            });
        }
        Ok(details)
    }

    pub async fn update(
        &self,
        id: &str,
        name: String,
        description: Option<String>,
    ) -> Result<Prompt, DomainError> {
        let prompt = self.repos.prompts.update(id, name, description).await?;
        self.audit("prompt.updated", id, json!({})).await;
        Ok(prompt)
    }

    pub async fn delete(&self, id: &str) -> Result<(), DomainError> {
        self.repos.prompts.delete(id).await?;
        self.audit("prompt.deleted", id, json!({})).await;
        Ok(())
    }

    pub async fn create_version(
        &self,
        prompt_id: &str,
        input: NewPromptVersion,
    ) -> Result<PromptVersion, DomainError> {
        let prompt = self.repos.prompts.get(prompt_id).await?;
        let version = self
            .repos
            .prompts
            .create_version(NewPromptVersion {
                prompt_id: prompt.id.clone(),
                ..input
            })
            .await?;
        self.audit(
            "prompt.version_created",
            &prompt.id,
            json!({"version": version.version}),
        )
        .await;
        Ok(version)
    }

    pub async fn publish(&self, version_id: &str) -> Result<PromptVersion, DomainError> {
        let version = self.repos.prompts.get_version(version_id).await?;
        // 旧 published → deprecated：生产引用可追溯（§18.1）
        if let Some(current) = self
            .repos
            .prompts
            .published_version(&version.prompt_id)
            .await?
        {
            if current.id != version.id {
                self.repos
                    .prompts
                    .set_version_status(&current.id, PromptVersionStatus::Deprecated)
                    .await?;
            }
        }
        let published = self
            .repos
            .prompts
            .set_version_status(version_id, PromptVersionStatus::Published)
            .await?;
        self.audit(
            "prompt.published",
            &version.prompt_id,
            json!({"version": published.version}),
        )
        .await;
        Ok(published)
    }

    pub async fn deprecate(&self, version_id: &str) -> Result<PromptVersion, DomainError> {
        let version = self
            .repos
            .prompts
            .set_version_status(version_id, PromptVersionStatus::Deprecated)
            .await?;
        self.audit(
            "prompt.deprecated",
            &version.prompt_id,
            json!({"version": version.version}),
        )
        .await;
        Ok(version)
    }

    /// 模板渲染：{{variable}} 替换；缺失变量用空串并记录在 warnings。
    pub fn render_template(
        template: &str,
        variables: &HashMap<String, Value>,
    ) -> (String, Vec<String>) {
        let mut warnings = Vec::new();
        let mut output = template.to_string();
        let mut start = 0;
        while let Some(i) = output[start..].find("{{") {
            let abs = start + i;
            if let Some(end_rel) = output[abs..].find("}}") {
                let end = abs + end_rel + 2;
                let key = output[abs + 2..abs + end_rel].trim().to_string();
                let value = variables.get(&key).map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
                match value {
                    Some(v) => output.replace_range(abs..end, &v),
                    None => {
                        warnings.push(key.clone());
                        output.replace_range(abs..end, "");
                    }
                }
                start = abs;
                if start >= output.len() {
                    break;
                }
            } else {
                break;
            }
        }
        (output, warnings)
    }

    /// 加载 published 版本为可执行配置（Playground / Agent 复用）。
    pub async fn resolve_published(
        &self,
        prompt_key: &str,
        variables: &HashMap<String, Value>,
    ) -> Result<ResolvedPrompt, DomainError> {
        let prompt = self.repos.prompts.get_by_key(prompt_key).await?;
        let version = self
            .repos
            .prompts
            .published_version(&prompt.id)
            .await?
            .ok_or_else(|| {
                DomainError::validation(
                    DomainResource::Prompt,
                    format!("prompt '{prompt_key}' has no published version"),
                )
            })?;
        let (system, mut warnings) = match &version.system_template {
            Some(t) => {
                let (s, w) = Self::render_template(t, variables);
                (Some(s), w)
            }
            None => (None, vec![]),
        };
        let (user, warnings2) = match &version.user_template {
            Some(t) => {
                let (s, w) = Self::render_template(t, variables);
                (Some(s), w)
            }
            None => (None, vec![]),
        };
        warnings.extend(warnings2);
        Ok(ResolvedPrompt {
            prompt_key: prompt.key.clone(),
            version,
            system,
            user,
            warnings,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedPrompt {
    pub prompt_key: String,
    pub version: PromptVersion,
    pub system: Option<String>,
    pub user: Option<String>,
    pub warnings: Vec<String>,
}
