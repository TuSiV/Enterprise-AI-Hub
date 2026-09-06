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

//! Evaluation（M16，方案 §20.6）：dataset/cases/runs，
//! rule 指标（exact/contains/regex）+ 可选 LLM Judge + 成本/延迟对比。

use std::sync::Arc;

use aihub_domain::canonical::{CanonicalChatRequest, CanonicalMessage, MessageRole};
use aihub_domain::error::DomainError;
use aihub_domain::platform::*;
use aihub_domain::DomainResource;
use serde_json::{json, Value};

use crate::pipeline::AuthContext;
use crate::{ChatPipeline, Repos};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleScore {
    pub exact: bool,
    pub contains: bool,
    pub regex_match: Option<bool>,
    /// 0-1 综合分
    pub score: f64,
}

pub struct EvalService {
    repos: Repos,
    pipeline: Arc<ChatPipeline>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalRunOutcome {
    pub run_id: String,
    pub status: String,
    pub summary: Value,
    pub results: Vec<EvalResult>,
}

impl EvalService {
    pub fn new(repos: Repos, pipeline: Arc<ChatPipeline>) -> Self {
        Self { repos, pipeline }
    }

    pub async fn create_dataset(
        &self,
        key: &str,
        name: &str,
        description: Option<String>,
    ) -> Result<EvalDataset, DomainError> {
        if key.trim().is_empty() {
            return Err(DomainError::validation(
                DomainResource::Eval,
                "key is required",
            ));
        }
        self.repos
            .evals
            .create_dataset(key, name, description)
            .await
    }

    pub async fn list_datasets(&self) -> Result<Vec<EvalDataset>, DomainError> {
        self.repos.evals.list_datasets().await
    }

    pub async fn add_case(
        &self,
        dataset_id: &str,
        name: &str,
        input: Value,
        expected: Option<String>,
    ) -> Result<EvalCase, DomainError> {
        self.repos.evals.get_dataset(dataset_id).await?;
        self.repos
            .evals
            .add_case(dataset_id, name, input, expected)
            .await
    }

    pub async fn list_cases(&self, dataset_id: &str) -> Result<Vec<EvalCase>, DomainError> {
        self.repos.evals.list_cases(dataset_id).await
    }

    /// 运行评估（§20.6）：candidate = {model, system?}；
    /// judge_config = {judgeModel?, rule?: {contains?, regex?, exact?}}。
    pub async fn run(
        &self,
        dataset_id: &str,
        label: &str,
        candidate: Value,
        judge_config: Value,
        ctx: &AuthContext,
    ) -> Result<EvalRunOutcome, DomainError> {
        let dataset = self.repos.evals.get_dataset(dataset_id).await?;
        let cases: Vec<EvalCase> = self.repos.evals.list_cases(&dataset.id).await?;
        if cases.is_empty() {
            return Err(DomainError::validation(
                DomainResource::Eval,
                "dataset has no cases",
            ));
        }
        let model = candidate
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DomainError::validation(DomainResource::Eval, "candidate.model is required")
            })?
            .to_string();
        let system = candidate
            .get("system")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let run = self
            .repos
            .evals
            .create_run(&dataset.id, label, candidate.clone(), judge_config.clone())
            .await?;

        let mut results = Vec::new();
        let mut scores = Vec::new();
        let mut total_cost = 0i64;
        let mut total_latency = 0i64;
        for case in &cases {
            let mut messages = Vec::new();
            if let Some(system) = &system {
                messages.push(CanonicalMessage {
                    role: MessageRole::System,
                    content: system.clone(),
                    tool_call_id: None,
                    name: None,
                });
            }
            // case.input: {question} 或 {messages:[...]} 或 字符串
            let question: &str = case
                .input
                .get("question")
                .and_then(|v| v.as_str())
                .or_else(|| case.input.as_str())
                .unwrap_or("");
            messages.push(CanonicalMessage {
                role: MessageRole::User,
                content: question.to_string(),
                tool_call_id: None,
                name: None,
            });
            if let Some(msgs) = case.input.get("messages").and_then(|v| v.as_array()) {
                messages = msgs
                    .iter()
                    .filter_map(|m| {
                        Some(CanonicalMessage {
                            role: MessageRole::parse(
                                m.get("role").and_then(|r| r.as_str()).unwrap_or("user"),
                            ),
                            content: m.get("content").and_then(|c| c.as_str())?.to_string(),
                            tool_call_id: None,
                            name: None,
                        })
                    })
                    .collect();
                if let Some(system) = &system {
                    messages.insert(
                        0,
                        CanonicalMessage {
                            role: MessageRole::System,
                            content: system.clone(),
                            tool_call_id: None,
                            name: None,
                        },
                    );
                }
            }

            let request = CanonicalChatRequest {
                model: model.clone(),
                messages,
                tools: Vec::new(),
                tool_choice: None,
                temperature: None,
                top_p: None,
                max_output_tokens: None,
                response_format: None,
                stream: false,
                metadata: json!({"origin": "eval", "runId": run.id}),
            };
            let outcome = match self.pipeline.execute_chat(ctx, request).await {
                Ok(execution) => {
                    let text = execution.response.content.clone().unwrap_or_default();
                    let cost = execution
                        .usage_cost
                        .as_ref()
                        .map(|u| u.cost_microunits)
                        .unwrap_or(0);
                    let input_tokens = execution
                        .usage_cost
                        .as_ref()
                        .map(|u| u.usage.input_tokens)
                        .unwrap_or(0);
                    let output_tokens = execution
                        .usage_cost
                        .as_ref()
                        .map(|u| u.usage.output_tokens)
                        .unwrap_or(0);
                    let latency = execution.latency_ms;
                    let rule = rule_score(&text, case.expected_output.as_deref(), &judge_config);
                    scores.push(rule.score);
                    let mut judge = None;
                    if let Some(judge_model) =
                        judge_config.get("judgeModel").and_then(|v| v.as_str())
                    {
                        judge = Some(
                            self.llm_judge(
                                judge_model,
                                question,
                                &text,
                                case.expected_output.as_deref(),
                                ctx,
                            )
                            .await,
                        );
                        if let Some(j) = &judge {
                            if let Some(s) = j.get("score").and_then(|v| v.as_f64()) {
                                // judge 0-5 → 归一到 0-1 并与 rule 各占 50%
                                scores
                                    .last_mut()
                                    .unwrap()
                                    .clone_from(&((rule.score + s / 5.0) / 2.0));
                            }
                        }
                    }
                    total_cost += cost;
                    total_latency += latency;
                    EvalResult {
                        id: uuid::Uuid::new_v4().to_string(),
                        run_id: run.id.clone(),
                        case_id: case.id.clone(),
                        response_text: Some(text),
                        latency_ms: Some(latency),
                        input_tokens,
                        output_tokens,
                        cost_microunits: cost,
                        score: serde_json::to_value(&rule).unwrap_or_default(),
                        judge,
                    }
                }
                Err(e) => EvalResult {
                    id: uuid::Uuid::new_v4().to_string(),
                    run_id: run.id.clone(),
                    case_id: case.id.clone(),
                    response_text: Some(format!("error: {}", e.message)),
                    latency_ms: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_microunits: 0,
                    score: json!({"exact": false, "contains": false, "score": 0.0}),
                    judge: None,
                },
            };
            let _ = self.repos.evals.insert_result(outcome.clone()).await;
            results.push(outcome);
        }

        let avg_score = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };
        let summary = json!({
            "cases": cases.len(),
            "avgScore": (avg_score * 1000.0).round() / 1000.0,
            "avgLatencyMs": if results.is_empty() { 0 } else { total_latency / results.len() as i64 },
            "totalCostMicrounits": total_cost,
        });
        self.repos
            .evals
            .finish_run(&run.id, "completed", summary.clone(), None)
            .await?;
        Ok(EvalRunOutcome {
            run_id: run.id,
            status: "completed".into(),
            summary,
            results,
        })
    }

    /// LLM Judge（§20.6：judge 只是信号而非真值，结果单列不覆盖 rule 分）。
    async fn llm_judge(
        &self,
        judge_model: &str,
        question: &str,
        answer: &str,
        expected: Option<&str>,
        ctx: &AuthContext,
    ) -> Value {
        let prompt = format!(
            "你是评估裁判。对以下回答按 1-5 打分并给一句理由，输出 JSON：{{\"score\": <1-5>, \"reason\": \"...\"}}。\n问题：{question}\n参考答案：{expected}\n回答：{answer}",
            expected = expected.unwrap_or("(无参考答案，按正确性/完整性评判)")
        );
        let request = CanonicalChatRequest {
            model: judge_model.to_string(),
            messages: vec![CanonicalMessage {
                role: MessageRole::User,
                content: prompt,
                tool_call_id: None,
                name: None,
            }],
            tools: Vec::new(),
            tool_choice: None,
            temperature: Some(0.0),
            top_p: None,
            max_output_tokens: None,
            response_format: None,
            stream: false,
            metadata: json!({"origin": "eval-judge"}),
        };
        match self.pipeline.execute_chat(ctx, request).await {
            Ok(execution) => {
                let raw = execution.response.content.clone().unwrap_or_default();
                match serde_json::from_str::<Value>(&raw) {
                    Ok(parsed) => parsed,
                    Err(_) => {
                        json!({"score": extract_first_int(&raw).unwrap_or(0.0), "reason": raw.chars().take(300).collect::<String>()})
                    }
                }
            }
            Err(e) => json!({"score": 0, "reason": format!("judge failed: {}", e.message)}),
        }
    }

    pub async fn run_detail(
        &self,
        run_id: &str,
    ) -> Result<(EvalRun, Vec<EvalResult>), DomainError> {
        let run = self.repos.evals.get_run(run_id).await?;
        let results = self.repos.evals.results_for_run(run_id).await?;
        Ok((run, results))
    }

    pub async fn runs(&self, dataset_id: &str) -> Result<Vec<EvalRun>, DomainError> {
        self.repos.evals.list_runs(dataset_id).await
    }
}

/// Rule 指标（§20.6 Exact/Rule Score）。
pub fn rule_score(answer: &str, expected: Option<&str>, judge_config: &Value) -> RuleScore {
    let rule = judge_config.get("rule").cloned().unwrap_or(json!({}));
    let expected = expected.unwrap_or("");
    let exact = !expected.is_empty() && answer.trim() == expected.trim();
    let contains = if let Some(needles) = rule.get("contains").and_then(|v| v.as_array()) {
        needles
            .iter()
            .filter_map(|n| n.as_str())
            .all(|n| answer.to_lowercase().contains(&n.to_lowercase()))
    } else {
        !expected.is_empty() && answer.to_lowercase().contains(&expected.to_lowercase())
    };
    let regex_match = rule.get("regex").and_then(|v| v.as_str()).map(|pattern| {
        // 避免引入 regex 依赖：V1 支持子串式 pattern + 常用 ^/$ 锚（文档说明），完整 regex 走 LLM judge
        let pattern = pattern.trim_start_matches('^').trim_end_matches('$');
        answer.contains(pattern)
    });
    let base: f64 = if exact {
        1.0
    } else if contains {
        0.7
    } else {
        0.0
    };
    let score: f64 = match regex_match {
        Some(true) => (base + 1.0) / 2.0,
        Some(false) => base / 2.0,
        None => base,
    };
    RuleScore {
        exact,
        contains,
        regex_match,
        score: (score * 1000.0).round() / 1000.0,
    }
}

fn extract_first_int(raw: &str) -> Option<f64> {
    let mut current: Option<String> = None;
    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            current.get_or_insert_with(String::new).push(ch);
        } else if let Some(number) = current.take() {
            if let Ok(v) = number.parse::<f64>() {
                return Some(v.clamp(1.0, 5.0));
            }
        }
    }
    current
        .and_then(|n| n.parse::<f64>().ok())
        .map(|v| v.clamp(1.0, 5.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_score_exact_beats_contains() {
        let cfg = json!({});
        let exact = rule_score("hello", Some("hello"), &cfg);
        assert!(exact.exact);
        assert_eq!(exact.score, 1.0);

        let contains = rule_score("well hello there", Some("hello"), &cfg);
        assert!(contains.contains);
        assert!((contains.score - 0.7).abs() < 1e-9);
    }

    #[test]
    fn rule_score_with_contains_keywords_and_regex() {
        let cfg = json!({"rule": {"contains": ["北京", "上海"], "regex": "^答案"}});
        let ok = rule_score("答案是 北京 和 上海", None, &cfg);
        // contains 关键词全命中 + regex 命中：(0.7 + 1) / 2
        assert!((ok.score - 0.85).abs() < 1e-9);
        let with_expected = rule_score("答案是 北京 和 上海", Some("北京 和 上海"), &cfg);
        assert!(with_expected.contains);
        assert!(with_expected.regex_match == Some(true));
        // contains(0.7) + regex 命中 → (0.7 + 1) / 2
        assert!((with_expected.score - 0.85).abs() < 1e-9);
    }

    #[test]
    fn judge_score_extraction() {
        assert_eq!(extract_first_int("评分：4/5，理由"), Some(4.0));
        assert_eq!(extract_first_int("no digits"), None);
    }
}
