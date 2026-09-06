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

//! Security Governance（M17，方案 §29）：
//! 数据分级→模型策略、DLP 规则链（detect→classify→allow/mask/block）、SSRF 目标校验。

use aihub_domain::error::DomainError;
use aihub_domain::platform::*;
use aihub_domain::DomainResource;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::Repos;

/// 数据分级（§29.5）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Strict,
}

impl DataClassification {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_uppercase().as_str() {
            "PUBLIC" => Some(DataClassification::Public),
            "INTERNAL" => Some(DataClassification::Internal),
            "CONFIDENTIAL" => Some(DataClassification::Confidential),
            "STRICT" => Some(DataClassification::Strict),
            _ => None,
        }
    }

    /// 默认策略矩阵（§29.5）：STRICT → private/local provider only。
    pub fn provider_kinds_allowed(&self) -> &'static [&'static str] {
        match self {
            DataClassification::Public => &[
                "openai",
                "openai_compatible",
                "anthropic",
                "gemini",
                "ollama",
            ],
            DataClassification::Internal => &["openai_compatible", "ollama", "openai"],
            DataClassification::Confidential => &["openai_compatible", "ollama"],
            DataClassification::Strict => &["ollama"],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlpVerdict {
    pub action: String, // allow / mask / block
    pub hits: Vec<DlpHit>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlpHit {
    pub rule: String,
    pub action: String,
    pub count: usize,
}

pub struct PolicyService {
    repos: Repos,
    /// desktop 模式允许 loopback（本地 provider/工具），Server 模式默认禁止
    pub allow_loopback: bool,
}

impl PolicyService {
    pub fn new(repos: Repos, allow_loopback: bool) -> Self {
        Self {
            repos,
            allow_loopback,
        }
    }

    async fn audit(&self, event: &str, decision: &str, metadata: serde_json::Value) {
        let _ = self
            .repos
            .audit
            .insert(aihub_domain::entities::AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                trace_id: None,
                actor_type: "policy".into(),
                actor_id: None,
                event_type: event.into(),
                resource_type: Some("security".into()),
                resource_id: None,
                decision: Some(decision.into()),
                payload_ref: None,
                metadata,
                created_at: chrono::Utc::now(),
            })
            .await;
    }

    pub async fn upsert_routing_policy(&self, policy: RoutingPolicy) -> Result<(), DomainError> {
        self.repos.policies.upsert_routing_policy(&policy).await?;
        self.audit(
            "policy.routing_upserted",
            "allow",
            json!({"key": policy.key}),
        )
        .await;
        Ok(())
    }

    pub async fn upsert_security_policy(&self, policy: SecurityPolicy) -> Result<(), DomainError> {
        self.repos.policies.upsert_security_policy(&policy).await?;
        self.audit(
            "policy.security_upserted",
            "allow",
            json!({"key": policy.key}),
        )
        .await;
        Ok(())
    }

    pub async fn list_routing(&self) -> Result<Vec<RoutingPolicy>, DomainError> {
        self.repos.policies.list_routing_policies().await
    }

    pub async fn list_security(
        &self,
        policy_type: Option<&str>,
    ) -> Result<Vec<SecurityPolicy>, DomainError> {
        self.repos
            .policies
            .list_security_policies(policy_type)
            .await
    }

    pub async fn delete_security(&self, id: &str) -> Result<(), DomainError> {
        self.repos.policies.delete_security_policy(id).await
    }

    pub async fn delete_routing(&self, id: &str) -> Result<(), DomainError> {
        self.repos.policies.delete_routing_policy(id).await
    }

    /// 数据分级 → 路由约束（§29.5/§43 验收：STRICT 只能到允许 provider kinds）。
    pub fn check_data_classification(
        &self,
        classification: DataClassification,
        provider_kind: &str,
    ) -> Result<(), DomainError> {
        check_classification(classification, provider_kind)
    }

    /// DLP（§29.6）：规则链 evaluate → allow/mask/block。V1 正则退化为关键词匹配 + 常见模式内置。
    pub async fn dlp_scan(&self, content: &str) -> Result<DlpVerdict, DomainError> {
        let policies = self
            .repos
            .policies
            .list_security_policies(Some("dlp"))
            .await?;
        let mut rules: Vec<DlpRule> = policies
            .iter()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                serde_json::from_value::<Vec<DlpRule>>(p.rule.clone()).unwrap_or_default()
            })
            .collect();
        // 内置基线规则：手机号/身份证模式（简单形态），生产可扩展完整正则引擎
        rules.push(DlpRule {
            name: "builtin-phone".into(),
            pattern: "phone".into(),
            kind: "builtin".into(),
            action: "mask".into(),
        });
        rules.push(DlpRule {
            name: "builtin-secret-token".into(),
            pattern: "token".into(),
            kind: "builtin".into(),
            action: "mask".into(),
        });

        let mut masked = content.to_string();
        let mut hits = Vec::new();
        let mut blocked = false;
        for rule in &rules {
            let count = match rule.kind.as_str() {
                "builtin" => match rule.name.as_str() {
                    "builtin-phone" => count_phone_like(&masked),
                    "builtin-secret-token" => count_secret_like(&masked),
                    _ => 0,
                },
                _ => masked.matches(&rule.pattern).count(),
            };
            if count == 0 {
                continue;
            }
            hits.push(DlpHit {
                rule: rule.name.clone(),
                action: rule.action.clone(),
                count,
            });
            match rule.action.as_str() {
                "block" => blocked = true,
                "mask" => match rule.kind.as_str() {
                    "builtin" => match rule.name.as_str() {
                        "builtin-phone" => masked = mask_phone_like(&masked),
                        "builtin-secret-token" => masked = mask_secret_like(&masked),
                        _ => {}
                    },
                    _ => masked = masked.replace(&rule.pattern, "***"),
                },
                _ => {}
            }
        }
        let action = if blocked {
            "block"
        } else if hits.is_empty() {
            "allow"
        } else {
            "mask"
        };
        if !hits.is_empty() {
            self.audit("policy.dlp", action, json!({"hits": hits}))
                .await;
        }
        Ok(DlpVerdict {
            action: action.into(),
            hits,
            content: masked,
        })
    }
}

/// SSRF（§29.3）：自定义 Provider/Tool/MCP endpoint 的目标校验。
pub fn assert_url_allowed(url: &str, allow_loopback: bool) -> Result<(), DomainError> {
    let lower = url.to_lowercase();
    let host = extract_host(url);
    let loopback = matches!(
        host.as_str(),
        "localhost" | "127.0.0.1" | "[::1]" | "0.0.0.0" | "::1" | ""
    ) || host.starts_with("127.")
        || host.starts_with("169.254.")   // link-local
        || host.starts_with("100.100.100.") // 云 metadata (aliyun)
        || host == "169.254.169.254"
        || host == "metadata.google.internal";
    if loopback && !allow_loopback {
        return Err(DomainError::validation(
            DomainResource::Policy,
            format!("url target '{host}' is blocked by SSRF policy (loopback/link-local/metadata)"),
        ));
    }
    let _ = lower;
    Ok(())
}

fn extract_host(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host_port = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    host_port
        .rsplit_once(':')
        .map(|(h, _)| h.to_string())
        .unwrap_or_else(|| host_port.to_string())
}

fn count_phone_like(content: &str) -> usize {
    let mut count = 0;
    for start in 0..content.len() {
        if !content.is_char_boundary(start) || start + 11 > content.len() {
            continue;
        }
        if !content.is_char_boundary(start + 11) {
            continue;
        }
        let window = &content[start..start + 11];
        if window.bytes().all(|b| b.is_ascii_digit())
            && (window.starts_with("13") || window.starts_with("15") || window.starts_with("18"))
        {
            count += 1;
        }
    }
    count
}

fn mask_phone_like(content: &str) -> String {
    let mut out = content.to_string();
    let mut start = 0;
    while start + 11 <= out.len() {
        if !out.is_char_boundary(start) || !out.is_char_boundary(start + 11) {
            start += 1;
            continue;
        }
        let window = &out[start..start + 11];
        if window.bytes().all(|b| b.is_ascii_digit())
            && (window.starts_with("13") || window.starts_with("15") || window.starts_with("18"))
        {
            let mask_from = start + 3;
            let mask_to = start + 7;
            if out.is_char_boundary(mask_from) && out.is_char_boundary(mask_to) {
                out.replace_range(mask_from..mask_to, "****");
                start += 11;
                continue;
            }
        }
        start += 1;
    }
    out
}

fn count_secret_like(content: &str) -> usize {
    const PREFIXES: &[&str] = &["sk-", "aih_live_", "aih_test_", "Bearer "];
    PREFIXES.iter().map(|p| content.matches(p).count()).sum()
}

fn mask_secret_like(content: &str) -> String {
    let mut out = content.to_string();
    for prefix in ["sk-", "aih_live_", "aih_test_"] {
        if let Some(pos) = out.find(prefix) {
            let end = (pos + prefix.len() + 12).min(out.len());
            out.replace_range(pos + prefix.len()..end, "***");
        }
    }
    out
}

/// 数据分级核心判定（无 DB 依赖，便于单测）。
pub fn check_classification(
    classification: DataClassification,
    provider_kind: &str,
) -> Result<(), DomainError> {
    let allowed = classification.provider_kinds_allowed();
    if allowed.contains(&provider_kind) {
        Ok(())
    } else {
        Err(DomainError::validation(
            DomainResource::Policy,
            format!(
                "data classification {classification:?} cannot be routed to provider kind '{provider_kind}'; allowed: {allowed:?}"
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_blocks_cloud_providers() {
        assert!(check_classification(DataClassification::Strict, "openai").is_err());
        assert!(check_classification(DataClassification::Strict, "ollama").is_ok());
        assert!(check_classification(DataClassification::Public, "openai").is_ok());
        assert!(
            check_classification(DataClassification::Confidential, "openai_compatible").is_ok()
        );
    }

    #[test]
    fn ssrf_blocks_metadata_and_loopback_on_server() {
        assert!(assert_url_allowed("http://169.254.169.254/latest/meta-data", false).is_err());
        assert!(assert_url_allowed("http://localhost:11434/v1", false).is_err());
        assert!(assert_url_allowed("http://localhost:11434/v1", true).is_ok());
        assert!(assert_url_allowed("https://api.openai.com/v1", false).is_ok());
    }

    #[test]
    fn dlp_masking_helpers() {
        assert_eq!(count_phone_like("联系我 13812345678"), 1);
        let masked = mask_phone_like("联系我 13812345678");
        assert!(masked.contains("138****"));
        assert!(!masked.contains("13812345678"));
        assert_eq!(count_secret_like("key sk-abc123456789"), 1);
    }
}
