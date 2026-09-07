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

//! Gateway 请求执行流水线（方案 §12.1 固定执行顺序）：
//! authenticate → authorize → validate → quota/rate-limit → resolve →
//! execute (retry/failover, circuit breaker) → usage → cost → persist → audit。

use std::sync::Arc;
use std::time::Instant;

use aihub_domain::canonical::{
    CanonicalChatRequest, CanonicalChatResponse, CanonicalUsage, StreamEvent, UsageSource,
};
use aihub_domain::cost;
use aihub_domain::entities::{Application, Model, RequestStatus};
use aihub_domain::repos::{NewAiRequest, RequestFinish};
use chrono::{Datelike, TimeZone};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::apikey;
use crate::breaker::{BreakerDecision, CircuitBreakerRegistry};
use crate::error::{GatewayCode, PipelineError};
use crate::limiter::{LimiterDecision, RateLimiter};
use crate::registry::ProviderRegistry;
use crate::resolver::{ModelResolver, ResolvedRoute, RouteCandidate};
use crate::Repos;

#[derive(Clone)]
pub struct AuthContext {
    pub application: Application,
    pub api_key_id: Option<String>,
    pub actor_type: &'static str,
    /// 终端用户标识：OpenAI `user` 字段或 `X-AiHub-User` 头（方案 §12：按用户归因）
    pub user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UsageCost {
    pub usage: CanonicalUsage,
    pub cost_microunits: i64,
    pub currency: String,
    pub pricing_snapshot: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatExecution {
    pub request_id: String,
    pub trace_id: String,
    pub response: CanonicalChatResponse,
    pub usage_cost: Option<UsageCost>,
    pub resolved_model_key: String,
    pub provider_key: String,
    pub virtual_model_key: Option<String>,
    pub retry_count: i32,
    pub latency_ms: i64,
}

/// 流式执行事件（由 Gateway 映射为 OpenAI chunk SSE）。
#[derive(Debug, Clone)]
pub enum PipelineStreamEvent {
    Started {
        resolved_model_key: String,
        provider_key: String,
        virtual_model_key: Option<String>,
    },
    Content {
        delta: String,
    },
    Reasoning {
        delta: String,
    },
    ToolCallStarted {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallArguments {
        index: usize,
        delta: String,
    },
    Completed {
        resolved_model_key: String,
        provider_key: String,
        usage_cost: Option<UsageCost>,
        latency_ms: i64,
        ttft_ms: i64,
        retry_count: i32,
    },
    Failed {
        code: GatewayCode,
        message: String,
    },
}

pub struct StreamExecution {
    pub request_id: String,
    pub trace_id: String,
    pub rx: mpsc::Receiver<PipelineStreamEvent>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
struct RetryConfig {
    #[serde(rename = "maxAttemptsPerTarget")]
    max_attempts_per_target: Option<u32>,
    #[serde(rename = "retry429")]
    retry_429: Option<bool>,
    #[serde(rename = "retry5xx")]
    retry_5xx: Option<bool>,
    #[serde(rename = "retryConnection")]
    retry_connection: Option<bool>,
    #[serde(rename = "retryTimeout")]
    retry_timeout: Option<bool>,
}

impl RetryConfig {
    fn from_config(value: &serde_json::Value) -> Self {
        value
            .get("retry")
            .map(|r| serde_json::from_value(r.clone()).unwrap_or_default())
            .unwrap_or_default()
    }

    fn attempts(&self, fallback: i32) -> u32 {
        self.max_attempts_per_target
            .unwrap_or(fallback.max(1) as u32)
            .clamp(1, 10)
    }

    /// 默认策略（§12.2）：连接失败/5xx 自动重试；429 与超时默认关闭。
    fn allows(&self, category: aihub_domain::error::ErrorCategory) -> bool {
        use aihub_domain::error::ErrorCategory as C;
        match category {
            C::Connection => self.retry_connection.unwrap_or(true),
            C::Provider5xx => self.retry_5xx.unwrap_or(true),
            C::RateLimited => self.retry_429.unwrap_or(false),
            C::Timeout => self.retry_timeout.unwrap_or(false),
            _ => false,
        }
    }
}

struct Prepared {
    request_id: String,
    trace_id: String,
    route: ResolvedRoute,
    virtual_model_key: Option<String>,
    retry_config: RetryConfig,
}

pub struct ChatPipeline {
    repos: Repos,
    registry: Arc<ProviderRegistry>,
    resolver: Arc<ModelResolver>,
    limiter: Arc<RateLimiter>,
    breakers: Arc<CircuitBreakerRegistry>,
}

impl ChatPipeline {
    pub fn new(
        repos: Repos,
        registry: Arc<ProviderRegistry>,
        resolver: Arc<ModelResolver>,
        limiter: Arc<RateLimiter>,
        breakers: Arc<CircuitBreakerRegistry>,
    ) -> Self {
        Self {
            repos,
            registry,
            resolver,
            limiter,
            breakers,
        }
    }

    // ---------- 认证（步骤 4/5） ----------

    /// Bearer Application API Key 认证。撤销/过期的 Key 立即拒绝（§43.1）。
    pub async fn authenticate_key(&self, bearer: &str) -> Result<AuthContext, PipelineError> {
        let Some(prefix) = apikey::key_prefix(bearer) else {
            return Err(PipelineError::new(
                GatewayCode::Unauthorized,
                "invalid api key format",
            ));
        };
        let key = self
            .repos
            .api_keys
            .get_by_prefix(prefix)
            .await
            .map_err(|e| PipelineError::from_domain(&e))?
            .ok_or_else(|| PipelineError::new(GatewayCode::Unauthorized, "unknown api key"))?;

        if !apikey::verify_hash(bearer, &key.secret_hash) {
            return Err(PipelineError::new(
                GatewayCode::Unauthorized,
                "api key verification failed",
            ));
        }
        if !key.is_active(chrono::Utc::now()) {
            return Err(PipelineError::new(
                GatewayCode::Unauthorized,
                "api key is revoked or expired",
            ));
        }

        let application = self
            .repos
            .applications
            .get(&key.application_id)
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        if application.status != "active" {
            return Err(PipelineError::new(
                GatewayCode::Forbidden,
                "application is not active",
            ));
        }

        let key_id = key.id.clone();
        let repos = self.repos.clone();
        // last_used_at 异步补写，不阻塞请求路径
        tokio::spawn(async move {
            let _ = repos
                .api_keys
                .touch_last_used(&key_id, chrono::Utc::now())
                .await;
        });

        Ok(AuthContext {
            application,
            api_key_id: Some(key.id),
            actor_type: "application",
            user_id: None,
        })
    }

    // ---------- 预检（步骤 6-11） ----------

    async fn prepare(
        &self,
        ctx: &AuthContext,
        request: &CanonicalChatRequest,
    ) -> Result<Prepared, PipelineError> {
        if request.messages.is_empty() {
            return Err(PipelineError::invalid_request("messages must not be empty"));
        }

        // Quota / Rate Limit（步骤 8/9）
        let quota = self
            .repos
            .quota
            .get_for_subject("application", &ctx.application.id)
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        let rpm = quota.as_ref().and_then(|q| q.rpm);
        let daily = quota.as_ref().and_then(|q| q.daily_requests);
        match self.limiter.check(&ctx.application.id, rpm, daily).await {
            LimiterDecision::Allowed => {}
            LimiterDecision::RateLimited { retry_after_secs } => {
                return Err(PipelineError::new(
                    GatewayCode::RateLimited,
                    format!("request rate limit exceeded (retry after {retry_after_secs}s)"),
                ));
            }
            LimiterDecision::DailyQuotaExceeded => {
                return Err(PipelineError::new(
                    GatewayCode::QuotaExceeded,
                    "daily request quota exceeded",
                ));
            }
        }
        if let Some(policy) = &quota {
            if policy.monthly_cost_microunits.unwrap_or(0) > 0 {
                let spent = self
                    .repos
                    .usage
                    .monthly_cost_for_application(&ctx.application.id, month_start_utc())
                    .await
                    .map_err(|e| PipelineError::from_domain(&e))?;
                if spent >= policy.monthly_cost_microunits.unwrap_or(0) {
                    self.limiter.refund(&ctx.application.id).await;
                    return Err(PipelineError::new(
                        GatewayCode::QuotaExceeded,
                        "monthly cost quota exceeded",
                    ));
                }
            }
        }
        // Application 级月度预算
        if let Some(budget) = ctx.application.monthly_budget_microunits {
            if budget > 0 {
                let spent = self
                    .repos
                    .usage
                    .monthly_cost_for_application(&ctx.application.id, month_start_utc())
                    .await
                    .map_err(|e| PipelineError::from_domain(&e))?;
                if spent >= budget {
                    self.limiter.refund(&ctx.application.id).await;
                    return Err(PipelineError::new(
                        GatewayCode::QuotaExceeded,
                        "monthly budget exceeded",
                    ));
                }
            }
        }

        // 模型解析（步骤 11）
        let route = self
            .resolver
            .resolve(&request.model)
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        if let Err(message) = ModelResolver::authorize(&ctx.application, &route) {
            return Err(PipelineError::new(GatewayCode::ModelNotAllowed, message));
        }
        if route.candidates.is_empty() {
            return Err(PipelineError::new(
                GatewayCode::RouteNotFound,
                "no available route targets for model; bind models to the virtual model first",
            ));
        }

        let retry_config = route
            .virtual_model
            .as_ref()
            .map(|vm| RetryConfig::from_config(&vm.config))
            .unwrap_or_default();
        let virtual_model_key = route.virtual_model.as_ref().map(|vm| vm.key.clone());

        // 请求记录先建行（步骤 19 的记录主体）
        let request_id = uuid::Uuid::new_v4().to_string();
        let trace_id = uuid::Uuid::new_v4().to_string();
        let record = NewAiRequest {
            id: request_id.clone(),
            trace_id: trace_id.clone(),
            application_id: Some(ctx.application.id.clone()),
            user_id: ctx.user_id.clone(),
            api_key_id: ctx.api_key_id.clone(),
            endpoint: "/v1/chat/completions".to_string(),
            requested_model: request.model.clone(),
            metadata: json!({
                "stream": request.stream,
                "messageCount": request.messages.len(),
            }),
            started_at: chrono::Utc::now(),
        };
        self.repos
            .requests
            .create(record)
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;

        Ok(Prepared {
            request_id,
            trace_id,
            route,
            virtual_model_key,
            retry_config,
        })
    }

    // ---------- 非流式执行 ----------

    pub async fn execute_chat(
        &self,
        ctx: &AuthContext,
        request: CanonicalChatRequest,
    ) -> Result<ChatExecution, PipelineError> {
        let prepared = self.prepare(ctx, &request).await?;
        let started = Instant::now();

        let mut last_error: Option<PipelineError> = None;
        let mut retry_count: i32 = 0;
        let mut succeeded: Option<(RouteCandidate, CanonicalChatResponse)> = None;

        'candidates: for candidate in &prepared.route.candidates {
            if self
                .breakers
                .check(&candidate.provider.id, &candidate.model.model_key)
                .await
                == BreakerDecision::Open
            {
                last_error = Some(PipelineError::new(
                    GatewayCode::ProviderUnavailable,
                    format!(
                        "circuit breaker open for {}/{}",
                        candidate.provider.key, candidate.model.model_key
                    ),
                ));
                continue;
            }
            let attempts = prepared
                .retry_config
                .attempts(candidate.provider.max_retries);
            for attempt in 0..attempts {
                let provider = match self.registry.get_for(&candidate.provider).await {
                    Ok(p) => p,
                    Err(e) => {
                        last_error = Some(PipelineError::from_domain(&e));
                        break;
                    }
                };
                match provider.chat(request.clone()).await {
                    Ok(response) => {
                        self.breakers
                            .record_success(&candidate.provider.id, &candidate.model.model_key)
                            .await;
                        succeeded = Some((candidate.clone(), response));
                        break 'candidates;
                    }
                    Err(err) => {
                        self.breakers
                            .record_failure(&candidate.provider.id, &candidate.model.model_key)
                            .await;
                        let pipeline_err = PipelineError::from_provider_error(&err);
                        tracing::warn!(
                            target: "aihub::gateway",
                            provider = %candidate.provider.key,
                            model = %candidate.model.model_key,
                            attempt,
                            category = err.category.as_str(),
                            "provider call failed"
                        );
                        last_error = Some(pipeline_err);
                        if attempt + 1 < attempts && prepared.retry_config.allows(err.category) {
                            retry_count += 1;
                            continue;
                        }
                        break; // failover 到下一候选
                    }
                }
            }
        }

        let Some((candidate, response)) = succeeded else {
            let error = last_error.unwrap_or_else(|| {
                PipelineError::new(GatewayCode::RouteNotFound, "no candidate executed")
            });
            self.persist_failure(&prepared, error.code, &error.message, retry_count, started)
                .await;
            return Err(error);
        };

        // Usage（步骤 17）：provider 原始 usage 优先，缺失时本地估算并标记 estimated（§12.5）
        let output_chars = response
            .content
            .as_deref()
            .map(|c| c.chars().count())
            .unwrap_or(0);
        let usage = response
            .usage
            .clone()
            .unwrap_or_else(|| estimate_usage(&request, output_chars));
        let usage_cost = Some(build_usage_cost(&candidate.model, usage));

        let latency_ms = started.elapsed().as_millis() as i64;
        self.persist_success(
            &prepared,
            &candidate,
            &usage_cost,
            latency_ms,
            None,
            retry_count,
        )
        .await;

        Ok(ChatExecution {
            request_id: prepared.request_id,
            trace_id: prepared.trace_id,
            response,
            usage_cost,
            resolved_model_key: candidate.model.model_key.clone(),
            provider_key: candidate.provider.key.clone(),
            virtual_model_key: prepared.virtual_model_key,
            retry_count,
            latency_ms,
        })
    }

    // ---------- 流式执行 ----------

    pub async fn execute_chat_stream(
        &self,
        ctx: &AuthContext,
        request: CanonicalChatRequest,
    ) -> Result<StreamExecution, PipelineError> {
        let prepared = self.prepare(ctx, &request).await?;
        let (tx, rx) = mpsc::channel::<PipelineStreamEvent>(64);

        let repos = self.repos.clone();
        let registry = self.registry.clone();
        let breakers = self.breakers.clone();
        let candidates = prepared.route.candidates.clone();
        let virtual_model_key = prepared.virtual_model_key.clone();
        let retry_config = prepared.retry_config.clone();
        let request_id = prepared.request_id.clone();
        let request_for_stream = request;

        tokio::spawn(async move {
            let started = Instant::now();
            let mut last_error: Option<PipelineError> = None;
            let mut retry_count: i32 = 0;
            let mut client_cancelled = false;

            'candidates: for candidate in &candidates {
                if breakers
                    .check(&candidate.provider.id, &candidate.model.model_key)
                    .await
                    == BreakerDecision::Open
                {
                    last_error = Some(PipelineError::new(
                        GatewayCode::ProviderUnavailable,
                        format!(
                            "circuit breaker open for {}/{}",
                            candidate.provider.key, candidate.model.model_key
                        ),
                    ));
                    continue;
                }
                let attempts = retry_config.attempts(candidate.provider.max_retries);

                'attempts: for attempt in 0..attempts {
                    let provider = match registry.get_for(&candidate.provider).await {
                        Ok(p) => p,
                        Err(e) => {
                            last_error = Some(PipelineError::from_domain(&e));
                            break 'attempts;
                        }
                    };
                    let mut stream = match provider.chat_stream(request_for_stream.clone()).await {
                        Ok(s) => s,
                        Err(err) => {
                            breakers
                                .record_failure(&candidate.provider.id, &candidate.model.model_key)
                                .await;
                            last_error = Some(PipelineError::from_provider_error(&err));
                            if attempt + 1 < attempts && retry_config.allows(err.category) {
                                retry_count += 1;
                                continue 'attempts;
                            }
                            break 'attempts; // failover 到下一候选
                        }
                    };

                    // 已建流：逐事件转发；一旦产生内容不得静默切换模型重放（§12.2）
                    let mut content_emitted = false;
                    let mut ttft_ms: Option<i64> = None;
                    let mut output_chars: usize = 0;
                    let mut usage: Option<CanonicalUsage> = None;
                    let mut finish_reason: Option<String> = None;
                    let mut mid_stream_error: Option<(
                        aihub_domain::error::ErrorCategory,
                        PipelineError,
                    )> = None;

                    if tx
                        .send(PipelineStreamEvent::Started {
                            resolved_model_key: candidate.model.model_key.clone(),
                            provider_key: candidate.provider.key.clone(),
                            virtual_model_key: virtual_model_key.clone(),
                        })
                        .await
                        .is_err()
                    {
                        client_cancelled = true;
                        break 'candidates;
                    }

                    while let Some(event) = stream.next().await {
                        let forward = match event {
                            StreamEvent::ResponseStarted { .. } => None,
                            StreamEvent::ContentDelta { delta } => {
                                if ttft_ms.is_none() {
                                    ttft_ms = Some(started.elapsed().as_millis() as i64);
                                }
                                content_emitted = true;
                                output_chars += delta.chars().count();
                                Some(PipelineStreamEvent::Content { delta })
                            }
                            StreamEvent::ReasoningDelta { delta } => {
                                Some(PipelineStreamEvent::Reasoning { delta })
                            }
                            StreamEvent::ToolCallStarted { index, id, name } => {
                                Some(PipelineStreamEvent::ToolCallStarted { index, id, name })
                            }
                            StreamEvent::ToolCallArgumentsDelta { index, delta } => {
                                Some(PipelineStreamEvent::ToolCallArguments { index, delta })
                            }
                            StreamEvent::UsageUpdated { usage: u } => {
                                usage = Some(u);
                                None
                            }
                            StreamEvent::ResponseCompleted {
                                finish_reason: fr,
                                usage: u,
                            } => {
                                finish_reason = fr;
                                if u.is_some() {
                                    usage = u;
                                }
                                None
                            }
                            StreamEvent::ProviderError {
                                category,
                                message,
                                http_status,
                            } => {
                                breakers
                                    .record_failure(
                                        &candidate.provider.id,
                                        &candidate.model.model_key,
                                    )
                                    .await;
                                let err = aihub_provider_core::ProviderError {
                                    category,
                                    message,
                                    http_status,
                                    retry_after_ms: None,
                                };
                                let pipeline_err = PipelineError::from_provider_error(&err);
                                if content_emitted {
                                    // 已输出内容：不得重放，以 STREAM_INTERRUPTED 结束（§12.2）
                                    persist_stream_failure(
                                        &repos,
                                        &request_id,
                                        Some(candidate),
                                        &virtual_model_key,
                                        GatewayCode::StreamInterrupted,
                                        &pipeline_err.message,
                                        retry_count,
                                        started.elapsed().as_millis() as i64,
                                        ttft_ms,
                                        output_chars,
                                    )
                                    .await;
                                    let _ = tx
                                        .send(PipelineStreamEvent::Failed {
                                            code: GatewayCode::StreamInterrupted,
                                            message: pipeline_err.message,
                                        })
                                        .await;
                                    return;
                                }
                                mid_stream_error = Some((category, pipeline_err));
                                break;
                            }
                        };
                        if let Some(evt) = forward {
                            if tx.send(evt).await.is_err() {
                                // 客户端断开：记录 client_cancelled 并尽力持久化已产生的 usage（§12.4）
                                client_cancelled = true;
                                break 'candidates;
                            }
                        }
                    }

                    if let Some((category, err)) = mid_stream_error {
                        if attempt + 1 < attempts && retry_config.allows(category) {
                            retry_count += 1;
                            continue 'attempts;
                        }
                        last_error = Some(err);
                        continue 'candidates;
                    }

                    // 流正常结束：持久化 Completed
                    breakers
                        .record_success(&candidate.provider.id, &candidate.model.model_key)
                        .await;
                    let latency_ms = started.elapsed().as_millis() as i64;
                    let ttft = ttft_ms.unwrap_or(latency_ms);
                    let final_usage = usage
                        .unwrap_or_else(|| estimate_usage_chars(&request_for_stream, output_chars));
                    let usage_cost = build_usage_cost(&candidate.model, final_usage);
                    persist_stream_success(
                        &repos,
                        &request_id,
                        candidate,
                        &virtual_model_key,
                        &usage_cost,
                        latency_ms,
                        ttft,
                        retry_count,
                        &finish_reason,
                    )
                    .await;
                    let _ = tx
                        .send(PipelineStreamEvent::Completed {
                            resolved_model_key: candidate.model.model_key.clone(),
                            provider_key: candidate.provider.key.clone(),
                            usage_cost: Some(usage_cost),
                            latency_ms,
                            ttft_ms: ttft,
                            retry_count,
                        })
                        .await;
                    return;
                }
            }

            if client_cancelled {
                persist_client_cancelled(
                    &repos,
                    &request_id,
                    last_error.as_ref().map(|e| e.message.clone()),
                )
                .await;
                return;
            }

            let error = last_error.unwrap_or_else(|| {
                PipelineError::new(GatewayCode::RouteNotFound, "no candidate executed")
            });
            persist_stream_failure(
                &repos,
                &request_id,
                None,
                &virtual_model_key,
                error.code,
                &error.message,
                retry_count,
                started.elapsed().as_millis() as i64,
                None,
                0,
            )
            .await;
            let _ = tx
                .send(PipelineStreamEvent::Failed {
                    code: error.code,
                    message: error.message,
                })
                .await;
        });

        Ok(StreamExecution {
            request_id: prepared.request_id,
            trace_id: prepared.trace_id,
            rx,
        })
    }

    // ---------- 持久化与审计（步骤 19/20） ----------

    async fn persist_success(
        &self,
        prepared: &Prepared,
        candidate: &RouteCandidate,
        usage_cost: &Option<UsageCost>,
        latency_ms: i64,
        ttft_ms: Option<i64>,
        retry_count: i32,
    ) {
        let _ = self
            .repos
            .requests
            .finish(
                &prepared.request_id,
                RequestFinish {
                    status: RequestStatus::Completed,
                    http_status: Some(200),
                    completed_at: chrono::Utc::now(),
                    ttft_ms,
                    latency_ms: Some(latency_ms),
                    retry_count,
                    error_code: None,
                    error_message_safe: None,
                    resolved_model_id: Some(candidate.model.id.clone()),
                    resolved_model_key: Some(candidate.model.model_key.clone()),
                    provider_id: Some(candidate.provider.id.clone()),
                },
            )
            .await;
        if let Some(uc) = usage_cost {
            persist_usage_cost(&self.repos, &prepared.request_id, uc).await;
        }
        emit_audit(
            &self.repos,
            &prepared.trace_id,
            "request.completed",
            Some(json!({
                "virtualModel": prepared.virtual_model_key,
                "provider": candidate.provider.key,
                "model": candidate.model.model_key,
                "tokens": usage_cost.as_ref().map(|u| u.usage.total_tokens),
                "costMicrounits": usage_cost.as_ref().map(|u| u.cost_microunits),
                "latencyMs": latency_ms,
            })),
        )
        .await;
    }

    async fn persist_failure(
        &self,
        prepared: &Prepared,
        code: GatewayCode,
        message: &str,
        retry_count: i32,
        started: Instant,
    ) {
        let _ = self
            .repos
            .requests
            .finish(
                &prepared.request_id,
                RequestFinish {
                    status: RequestStatus::Failed,
                    http_status: Some(code.http_status() as i64),
                    completed_at: chrono::Utc::now(),
                    ttft_ms: None,
                    latency_ms: Some(started.elapsed().as_millis() as i64),
                    retry_count,
                    error_code: Some(code.as_str().to_string()),
                    error_message_safe: Some(truncate_safe_message(message)),
                    resolved_model_id: None,
                    resolved_model_key: None,
                    provider_id: None,
                },
            )
            .await;
        emit_audit(
            &self.repos,
            &prepared.trace_id,
            "request.failed",
            Some(json!({
                "errorCode": code.as_str(),
                "virtualModel": prepared.virtual_model_key,
            })),
        )
        .await;
    }
}

// ---------- 流式任务使用的持久化自由函数 ----------

#[allow(clippy::too_many_arguments)]
async fn persist_stream_success(
    repos: &Repos,
    request_id: &str,
    candidate: &RouteCandidate,
    virtual_model_key: &Option<String>,
    usage_cost: &UsageCost,
    latency_ms: i64,
    ttft_ms: i64,
    retry_count: i32,
    finish_reason: &Option<String>,
) {
    let _ = repos
        .requests
        .finish(
            request_id,
            RequestFinish {
                status: RequestStatus::Completed,
                http_status: Some(200),
                completed_at: chrono::Utc::now(),
                ttft_ms: Some(ttft_ms),
                latency_ms: Some(latency_ms),
                retry_count,
                error_code: None,
                error_message_safe: None,
                resolved_model_id: Some(candidate.model.id.clone()),
                resolved_model_key: Some(candidate.model.model_key.clone()),
                provider_id: Some(candidate.provider.id.clone()),
            },
        )
        .await;
    persist_usage_cost(repos, request_id, usage_cost).await;
    emit_audit(
        repos,
        &format!("stream-{request_id}"),
        "request.completed",
        Some(json!({
            "stream": true,
            "virtualModel": virtual_model_key,
            "provider": candidate.provider.key,
            "model": candidate.model.model_key,
            "tokens": usage_cost.usage.total_tokens,
            "costMicrounits": usage_cost.cost_microunits,
            "latencyMs": latency_ms,
            "finishReason": finish_reason,
        })),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn persist_stream_failure(
    repos: &Repos,
    request_id: &str,
    candidate: Option<&RouteCandidate>,
    virtual_model_key: &Option<String>,
    code: GatewayCode,
    message: &str,
    retry_count: i32,
    latency_ms: i64,
    ttft_ms: Option<i64>,
    output_chars: usize,
) {
    let _ = repos
        .requests
        .finish(
            request_id,
            RequestFinish {
                status: RequestStatus::Failed,
                http_status: Some(code.http_status() as i64),
                completed_at: chrono::Utc::now(),
                ttft_ms,
                latency_ms: Some(latency_ms),
                retry_count,
                error_code: Some(code.as_str().to_string()),
                error_message_safe: Some(truncate_safe_message(message)),
                resolved_model_id: candidate.map(|c| c.model.id.clone()),
                resolved_model_key: candidate.map(|c| c.model.model_key.clone()),
                provider_id: candidate.map(|c| c.provider.id.clone()),
            },
        )
        .await;
    // 中途失败但已有输出：以 estimated usage 入库，保证计量可追溯
    if output_chars > 0 {
        if let Some(candidate) = candidate {
            let usage = estimate_output_usage(output_chars);
            let usage_cost = build_usage_cost(&candidate.model, usage);
            persist_usage_cost(repos, request_id, &usage_cost).await;
        }
    }
    let _ = virtual_model_key;
    emit_audit(
        repos,
        &format!("stream-{request_id}"),
        "request.failed",
        Some(json!({
            "errorCode": code.as_str(),
            "stream": true,
        })),
    )
    .await;
}

async fn persist_client_cancelled(repos: &Repos, request_id: &str, message: Option<String>) {
    let _ = repos
        .requests
        .finish(
            request_id,
            RequestFinish {
                status: RequestStatus::ClientCancelled,
                http_status: Some(499),
                completed_at: chrono::Utc::now(),
                ttft_ms: None,
                latency_ms: None,
                retry_count: 0,
                error_code: Some("AIH_STREAM_INTERRUPTED".to_string()),
                error_message_safe: message.or_else(|| Some("client disconnected".to_string())),
                resolved_model_id: None,
                resolved_model_key: None,
                provider_id: None,
            },
        )
        .await;
    emit_audit(
        repos,
        &format!("stream-{request_id}"),
        "request.client_cancelled",
        None,
    )
    .await;
}

async fn persist_usage_cost(repos: &Repos, request_id: &str, usage_cost: &UsageCost) {
    let usage_record = aihub_domain::entities::UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        request_id: request_id.to_string(),
        input_tokens: usage_cost.usage.input_tokens,
        output_tokens: usage_cost.usage.output_tokens,
        cached_input_tokens: usage_cost.usage.cached_input_tokens,
        reasoning_tokens: usage_cost.usage.reasoning_tokens,
        total_tokens: usage_cost.usage.total_tokens,
        usage_source: match usage_cost.usage.source {
            UsageSource::Provider => "provider".to_string(),
            UsageSource::Estimated => "estimated".to_string(),
        },
        raw_usage: serde_json::to_value(&usage_cost.usage).unwrap_or_default(),
        created_at: chrono::Utc::now(),
    };
    if let Err(e) = repos.requests.insert_usage(usage_record).await {
        tracing::warn!(target: "aihub::gateway", request_id, error = %e, "failed to persist usage");
    }
    let cost_record = aihub_domain::entities::CostRecord {
        id: uuid::Uuid::new_v4().to_string(),
        request_id: request_id.to_string(),
        currency: usage_cost.currency.clone(),
        input_cost_microunits: 0,
        output_cost_microunits: 0,
        cache_cost_microunits: 0,
        reasoning_cost_microunits: 0,
        total_cost_microunits: usage_cost.cost_microunits,
        pricing_snapshot: usage_cost.pricing_snapshot.clone(),
        created_at: chrono::Utc::now(),
    };
    if let Err(e) = repos.requests.insert_cost(cost_record).await {
        tracing::warn!(target: "aihub::gateway", request_id, error = %e, "failed to persist cost");
    }
}

async fn emit_audit(
    repos: &Repos,
    trace_id: &str,
    event_type: &str,
    metadata: Option<serde_json::Value>,
) {
    let event = aihub_domain::entities::AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        trace_id: Some(trace_id.to_string()),
        actor_type: "gateway".to_string(),
        actor_id: None,
        event_type: event_type.to_string(),
        resource_type: Some("request".to_string()),
        resource_id: None,
        decision: None,
        payload_ref: None,
        metadata: metadata.unwrap_or(json!({})),
        created_at: chrono::Utc::now(),
    };
    if let Err(e) = repos.audit.insert(event).await {
        tracing::warn!(target: "aihub::gateway", error = %e, "failed to persist audit event");
    }
}

// ---------- 工具 ----------

fn build_usage_cost(model: &Model, usage: CanonicalUsage) -> UsageCost {
    let breakdown = cost::calculate(&model.pricing, &usage);
    UsageCost {
        usage,
        cost_microunits: breakdown.total_cost_microunits,
        currency: model.pricing.currency.clone(),
        pricing_snapshot: serde_json::to_value(&model.pricing).unwrap_or_default(),
    }
}

/// 输入 token 估算（chars/4）。仅在 Provider 未返回 usage 时使用，必须标记 estimated（§12.5）。
fn estimate_usage(request: &CanonicalChatRequest, output_chars: usize) -> CanonicalUsage {
    let input_chars: usize = request
        .messages
        .iter()
        .map(|m| m.content.chars().count())
        .sum();
    let input_tokens = (input_chars / 4).max(1) as i64;
    let output_tokens = (output_chars / 4).max(1) as i64;
    CanonicalUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: input_tokens + output_tokens,
        source: UsageSource::Estimated,
    }
}

fn estimate_usage_chars(_request: &CanonicalChatRequest, output_chars: usize) -> CanonicalUsage {
    estimate_output_usage(output_chars)
}

fn estimate_output_usage(output_chars: usize) -> CanonicalUsage {
    let output_tokens = (output_chars / 4).max(1) as i64;
    CanonicalUsage {
        input_tokens: 0,
        output_tokens,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: output_tokens,
        source: UsageSource::Estimated,
    }
}

fn truncate_safe_message(message: &str) -> String {
    message.chars().take(500).collect()
}

fn month_start_utc() -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    chrono::Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now)
}
