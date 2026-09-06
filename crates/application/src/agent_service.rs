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

//! Agent / Tool / MCP（M14/M15，方案 §20）：
//! Agent 循环执行（max steps / tool auth / tool 审计），Tool 统一执行器（builtin + http 只读 + mcp 映射）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use aihub_domain::canonical::{
    CanonicalChatRequest, CanonicalMessage, MessageRole, ToolDefinitionData,
};
use aihub_domain::error::DomainError;
use aihub_domain::platform::*;
use aihub_domain::DomainResource;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::pipeline::AuthContext;
use crate::{ChatPipeline, PipelineError, Repos};

pub struct ToolExecutor {
    repos: Repos,
    /// mcp server registry 的懒连接缓存（M15）：key → (transport, endpoint)
    mcp_cache: Arc<RwLock<HashMap<String, McpServer>>>,
    /// SSRF/权限白名单外的 URL 拒绝由 policy_service::assert_url_allowed 前置（M17）
    pub allow_loopback_http: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub status: String, // succeeded / failed / timeout
    pub content: String,
    pub structured_data: Option<Value>,
    pub error: Option<String>,
    pub latency_ms: i64,
}

impl ToolExecutor {
    pub fn new(repos: Repos) -> Self {
        Self {
            repos,
            mcp_cache: Arc::new(RwLock::new(HashMap::new())),
            allow_loopback_http: true,
        }
    }

    /// 统一 Tool 执行入口（§20.4）：builtin/http/mcp 共用权限、超时与审计由调用方（AgentService）负责。
    pub async fn execute(&self, tool: &Tool, arguments: &Value) -> ToolResult {
        let started = Instant::now();
        let result: Result<(String, Option<Value>), String> = match tool.kind.as_str() {
            "builtin" => self.execute_builtin(&tool.key, arguments).await,
            "http" => self.execute_http(tool, arguments).await,
            "mcp" => self.execute_mcp(tool, arguments).await,
            other => Err(format!("unknown tool kind '{other}'")),
        };
        let latency = started.elapsed().as_millis() as i64;
        if started.elapsed() > std::time::Duration::from_millis(tool.timeout_ms.max(1) as u64) {
            // 超时兜底标记（执行本身在 future 内无法强杀，由调用方 tokio::time::timeout 包裹）
        }
        match result {
            Ok((content, structured)) => ToolResult {
                status: "succeeded".into(),
                content,
                structured_data: structured,
                error: None,
                latency_ms: latency,
            },
            Err(e) => ToolResult {
                status: "failed".into(),
                content: String::new(),
                structured_data: None,
                error: Some(e),
                latency_ms: latency,
            },
        }
    }

    async fn execute_builtin(
        &self,
        key: &str,
        arguments: &Value,
    ) -> Result<(String, Option<Value>), String> {
        match key {
            "echo" => {
                let message = arguments
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                Ok((message.to_string(), Some(json!({"message": message}))))
            }
            "now" => Ok((
                chrono::Utc::now().to_rfc3339(),
                Some(json!({"now": chrono::Utc::now().to_rfc3339()})),
            )),
            "kb_search" => {
                Err("kb_search requires KnowledgeService; bind it at the host layer".into())
            }
            other => Err(format!("unknown builtin tool '{other}'")),
        }
    }

    /// HTTP Tool：默认仅允许 GET（只读，§20.3 高风险写操作需显式授权）。
    async fn execute_http(
        &self,
        tool: &Tool,
        arguments: &Value,
    ) -> Result<(String, Option<Value>), String> {
        let url = tool
            .config
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                arguments
                    .get("url")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
            .ok_or_else(|| "http tool requires url in config or arguments".to_string())?;
        let method = tool
            .config
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        if method != "GET" {
            return Err(format!(
                "http tool method '{method}' is not allowed (read-only policy)"
            ));
        }
        crate::policy_service::assert_url_allowed(&url, self.allow_loopback_http)
            .map_err(|e| e.message)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                tool.timeout_ms.max(1) as u64
            ))
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let truncated: String = body.chars().take(4000).collect();
        Ok((
            truncated.clone(),
            Some(json!({"status": status, "bodyPreview": truncated})),
        ))
    }

    /// MCP stdio transport：spawn 命令，换行分隔 JSON-RPC（MCP stdio 规范）。
    async fn execute_mcp_stdio(
        &self,
        server: &McpServer,
        tool: &Tool,
        arguments: &Value,
    ) -> Result<(String, Option<Value>), String> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&server.endpoint_or_command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("mcp stdio spawn failed: {e}"))?;
        let mut stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let request_id = 1;
        let payload = serde_json::json!({
            "jsonrpc": "2.0", "id": request_id, "method": "tools/call",
            "params": {
                "name": tool.config.get("remoteTool").and_then(|v| v.as_str()).unwrap_or(&tool.key),
                "arguments": arguments,
            }
        });
        stdin
            .write_all(format!("{payload}\n").as_bytes())
            .await
            .map_err(|e| format!("mcp stdio write failed: {e}"))?;
        let _ = stdin.shutdown().await;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(tool.timeout_ms.max(1) as u64);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                let _ = child.kill().await;
                return Err("mcp stdio response timeout".into());
            }
            match tokio::time::timeout(remaining, reader.read_line(&mut line)).await {
                Err(_) => {
                    let _ = child.kill().await;
                    return Err("mcp stdio response timeout".into());
                }
                Ok(Err(e)) => return Err(format!("mcp stdio read failed: {e}")),
                Ok(Ok(0)) => return Err("mcp stdio closed without response".into()),
                Ok(Ok(_)) => {}
            }
            let trimmed = line.trim().to_string();
            line.clear();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(body) = serde_json::from_str::<Value>(&trimmed) else {
                continue;
            };
            if body.get("id").and_then(|v| v.as_i64()) != Some(request_id) {
                continue;
            }
            if let Some(error) = body.get("error") {
                let _ = child.kill().await;
                return Err(format!("mcp error: {error}"));
            }
            let content = body
                .pointer("/result/content/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let result = body.pointer("/result").cloned().unwrap_or(json!({}));
            let _ = child.kill().await;
            return Ok((content, Some(result)));
        }
    }

    /// MCP 工具发现（§20.5 tools/list）：把远端工具映射为平台 ToolDefinition（kind=mcp）。
    pub async fn discover_mcp_tools(&self, server_id: &str) -> Result<Vec<Tool>, DomainError> {
        let server = self.repos.mcp_servers.get(server_id).await?;
        let remote: Vec<(String, String)> = if server.transport == "streamable-http" {
            crate::policy_service::assert_url_allowed(
                &server.endpoint_or_command,
                self.allow_loopback_http,
            )
            .map_err(|e| DomainError::internal(DomainResource::Tool, e.message))?;
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|e| DomainError::internal(DomainResource::Tool, e.to_string()))?;
            let response = client
                .post(&server.endpoint_or_command)
                .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}))
                .send()
                .await
                .map_err(|e| {
                    DomainError::internal(
                        DomainResource::Tool,
                        format!("mcp tools/list failed: {e}"),
                    )
                })?;
            let body: Value = response.json().await.map_err(|e| {
                DomainError::internal(
                    DomainResource::Tool,
                    format!("mcp response parse failed: {e}"),
                )
            })?;
            body.pointer("/result/tools")
                .and_then(|t| t.as_array())
                .map(|tools| {
                    tools
                        .iter()
                        .filter_map(|t| {
                            Some((
                                t.get("name")?.as_str()?.to_string(),
                                t.get("description")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            // stdio 发现走同一换行 JSON-RPC 协议
            let tool_stub = Tool {
                id: String::new(),
                key: "__discover".into(),
                name: String::new(),
                description: None,
                kind: "mcp".into(),
                input_schema: json!({}),
                config: json!({}),
                timeout_ms: 15000,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let _ = &tool_stub;
            vec![] // stdio 发现：连接生命周期由 server 常驻进程管理，V1 支持常驻 server 的 tools/list 略——文档标注
        };
        let mut created = Vec::new();
        for (name, description) in remote {
            let key = format!("{}_{}", server.key, name);
            if self.repos.tools.get_by_key(&key).await.is_ok() {
                continue;
            }
            let tool = self
                .repos
                .tools
                .create(NewTool {
                    key,
                    name: format!("{}·{name}", server.name),
                    description: Some(description),
                    kind: "mcp".into(),
                    input_schema: json!({"type": "object"}),
                    config: json!({"serverKey": server.key, "remoteTool": name}),
                    timeout_ms: server_transport_timeout(&server),
                })
                .await?;
            created.push(tool);
        }
        Ok(created)
    }

    /// MCP Tool（M15）：streamable-http transport 的最小 JSON-RPC 子集
    /// （initialize → tools/call），复用平台权限与审计（§20.5）。
    async fn execute_mcp(
        &self,
        tool: &Tool,
        arguments: &Value,
    ) -> Result<(String, Option<Value>), String> {
        let server_key = tool
            .config
            .get("serverKey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "mcp tool requires serverKey in config".to_string())?;
        let cache = self.mcp_cache.read().await;
        let server = cache.get(server_key).cloned();
        drop(cache);
        let server = match server {
            Some(s) => Some(s),
            None => {
                let fetched = self.repos.mcp_servers.get_by_key(server_key).await.ok();
                if let Some(s) = &fetched {
                    self.mcp_cache
                        .write()
                        .await
                        .insert(server_key.to_string(), s.clone());
                }
                fetched
            }
        }
        .ok_or_else(|| format!("mcp server '{server_key}' not found"))?;
        if !server.enabled {
            return Err(format!("mcp server '{server_key}' is disabled"));
        }
        if server.transport == "stdio" {
            return self.execute_mcp_stdio(&server, tool, arguments).await;
        }
        if server.transport != "streamable-http" {
            return Err(format!(
                "mcp transport '{}' not supported (stdio / streamable-http)",
                server.transport
            ));
        }
        crate::policy_service::assert_url_allowed(
            &server.endpoint_or_command,
            self.allow_loopback_http,
        )
        .map_err(|e| e.message)?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                tool.timeout_ms.max(1) as u64
            ))
            .build()
            .map_err(|e| e.to_string())?;
        // JSON-RPC tools/call
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool.config.get("remoteTool").and_then(|v| v.as_str()).unwrap_or(&tool.key),
                "arguments": arguments,
            }
        });
        let response = client
            .post(&server.endpoint_or_command)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("mcp request failed: {e}"))?;
        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("mcp response parse failed: {e}"))?;
        if let Some(error) = body.get("error") {
            return Err(format!("mcp error: {error}"));
        }
        let content = body
            .pointer("/result/content/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok((
            content,
            Some(body.pointer("/result").cloned().unwrap_or(json!({}))),
        ))
    }
}

pub struct AgentService {
    repos: Repos,
    pipeline: Arc<ChatPipeline>,
    tools: Arc<ToolExecutor>,
}

/// 单步循环的每步产出（供 UI 流式展示走简化路径：V1 run 为同步聚合结果）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResult {
    pub run_id: String,
    pub status: String,
    pub steps: i32,
    pub output: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub error: Option<String>,
    pub cost_microunits: i64,
}

impl AgentService {
    pub fn new(repos: Repos, pipeline: Arc<ChatPipeline>, tools: Arc<ToolExecutor>) -> Self {
        Self {
            repos,
            pipeline,
            tools,
        }
    }

    pub async fn create(
        &self,
        key: &str,
        name: &str,
        description: Option<String>,
        version: NewAgentVersion,
    ) -> Result<(Agent, AgentVersion), DomainError> {
        if key.trim().is_empty() {
            return Err(DomainError::validation(
                DomainResource::Agent,
                "key is required",
            ));
        }
        let pair = self
            .repos
            .agents
            .create(
                NewAgent {
                    key: key.to_string(),
                    name: name.to_string(),
                    description,
                },
                NewAgentVersion {
                    status: "draft".into(),
                    ..version
                },
            )
            .await?;
        self.audit("agent.created", &pair.0.id, json!({"key": key}))
            .await;
        Ok(pair)
    }

    pub async fn create_version(
        &self,
        agent_id: &str,
        version: NewAgentVersion,
    ) -> Result<AgentVersion, DomainError> {
        self.repos.agents.get(agent_id).await?;
        let v = self
            .repos
            .agents
            .create_version(NewAgentVersion {
                status: "draft".into(),
                ..version
            })
            .await?;
        self.audit(
            "agent.version_created",
            agent_id,
            json!({"version": v.version}),
        )
        .await;
        Ok(v)
    }

    /// 发布：Published 版本不可原地修改（§39.4），修改必须新版本。
    pub async fn publish(&self, version_id: &str) -> Result<AgentVersion, DomainError> {
        let version = self.repos.agents.get_version(version_id).await?;
        if let Some(current) = self
            .repos
            .agents
            .published_version(&version.agent_id)
            .await?
        {
            if current.id != version.id {
                self.repos
                    .agents
                    .set_version_status(&current.id, "deprecated")
                    .await?;
            }
        }
        self.repos
            .agents
            .set_version_status(version_id, "published")
            .await?;
        self.audit(
            "agent.published",
            &version.agent_id,
            json!({"version": version.version}),
        )
        .await;
        self.repos.agents.get_version(version_id).await
    }

    pub async fn deprecate(&self, version_id: &str) -> Result<(), DomainError> {
        self.repos
            .agents
            .set_version_status(version_id, "deprecated")
            .await?;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<Agent>, DomainError> {
        self.repos.agents.list().await
    }

    pub async fn versions(&self, agent_id: &str) -> Result<Vec<AgentVersion>, DomainError> {
        self.repos.agents.versions_for(agent_id).await
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>, DomainError> {
        self.repos.tools.list().await
    }

    pub async fn create_tool(&self, tool: NewTool) -> Result<Tool, DomainError> {
        let tool = self.repos.tools.create(tool).await?;
        self.audit("tool.created", &tool.id, json!({"key": tool.key}))
            .await;
        Ok(tool)
    }

    pub async fn delete_tool(&self, id: &str) -> Result<(), DomainError> {
        self.repos.tools.delete(id).await?;
        Ok(())
    }

    pub async fn create_mcp_server(&self, server: NewMcpServer) -> Result<McpServer, DomainError> {
        let server = self.repos.mcp_servers.create(server).await?;
        self.audit("mcp_server.created", &server.id, json!({"key": server.key}))
            .await;
        Ok(server)
    }

    pub async fn list_mcp_servers(&self) -> Result<Vec<McpServer>, DomainError> {
        self.repos.mcp_servers.list().await
    }

    pub async fn delete_mcp_server(&self, id: &str) -> Result<(), DomainError> {
        self.repos.mcp_servers.delete(id).await?;
        Ok(())
    }

    /// Agent 运行（§20.1）：LLM → tool_calls? → authorize → execute → append → next step。
    pub async fn run_published(
        &self,
        agent_key: &str,
        input: Value,
        ctx: &AuthContext,
    ) -> Result<AgentRunResult, DomainError> {
        let agent = self.repos.agents.get_by_key(agent_key).await?;
        let version = self
            .repos
            .agents
            .published_version(&agent.id)
            .await?
            .ok_or_else(|| {
                DomainError::validation(
                    DomainResource::Agent,
                    format!("agent '{agent_key}' has no published version"),
                )
            })?;

        let run = self
            .repos
            .agents
            .create_run(NewAgentRun {
                agent_id: agent.id.clone(),
                agent_version_id: version.id.clone(),
                trace_id: Some(uuid::Uuid::new_v4().to_string()),
                max_steps: version.max_steps,
                input: input.clone(),
                metadata: json!({"actor": ctx.actor_type, "application": ctx.application.key}),
            })
            .await?;

        let result = self.execute_run(&run.id, &version, input, ctx).await;
        match result {
            Ok(r) => Ok(r),
            Err(e) => {
                let _ = self
                    .repos
                    .agents
                    .finish_run(
                        &run.id,
                        "failed",
                        None,
                        Some("AGENT_FAILED".into()),
                        Some(e.message.clone()),
                        0,
                    )
                    .await;
                Err(e)
            }
        }
    }

    async fn execute_run(
        &self,
        run_id: &str,
        version: &AgentVersion,
        input: Value,
        ctx: &AuthContext,
    ) -> Result<AgentRunResult, DomainError> {
        // 组装工具定义（allowedTools 白名单交集，§20.3）
        let enabled_tools = self.repos.tools.list_enabled().await?;
        let tools: Vec<&Tool> = enabled_tools
            .iter()
            .filter(|t| {
                version.allowed_tools.is_empty()
                    || version.allowed_tools.iter().any(|k| k == &t.key)
            })
            .collect();
        let definitions: Vec<ToolDefinitionData> = tools
            .iter()
            .map(|t| ToolDefinitionData {
                name: t.key.clone(),
                description: t.description.clone(),
                parameters: Some(t.input_schema.clone()),
            })
            .collect();

        let mut messages = vec![CanonicalMessage {
            role: MessageRole::User,
            content: input
                .get("task")
                .and_then(|v| v.as_str())
                .or_else(|| input.as_str())
                .unwrap_or("完成用户任务")
                .to_string(),
            tool_call_id: None,
            name: None,
        }];
        if let Some(system) = &version.system_prompt {
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

        let mut total_cost = 0i64;
        let mut tool_call_records: Vec<ToolCall> = Vec::new();
        let mut tool_calls_count = 0i32;
        let mut final_output: Option<String> = None;
        let mut step = 0;

        while step < version.max_steps {
            step += 1;
            self.repos
                .agents
                .finish_run(run_id, "running", None, None, None, total_cost)
                .await?;
            let request = CanonicalChatRequest {
                model: version.model_ref.clone(),
                messages: messages.clone(),
                tools: definitions.clone(),
                tool_choice: None,
                temperature: None,
                top_p: None,
                max_output_tokens: None,
                response_format: None,
                stream: false,
                metadata: json!({"agentRunId": run_id, "step": step}),
            };
            let execution =
                self.pipeline
                    .execute_chat(ctx, request)
                    .await
                    .map_err(|e: PipelineError| {
                        DomainError::internal(
                            DomainResource::Agent,
                            format!("model call failed: {}", e.message),
                        )
                    })?;
            if let Some(uc) = &execution.usage_cost {
                total_cost += uc.cost_microunits;
            }
            if let Some(max_cost) = version.max_cost_microunits {
                if total_cost > max_cost {
                    let _ = self
                        .repos
                        .agents
                        .finish_run(
                            run_id,
                            "failed",
                            None,
                            Some("AGENT_MAX_COST".into()),
                            Some("cost guard exceeded".into()),
                            total_cost,
                        )
                        .await;
                    return Err(DomainError::validation(
                        DomainResource::Agent,
                        "agent max cost exceeded",
                    ));
                }
            }

            let response = &execution.response;
            if response.tool_calls.is_empty() {
                final_output = response.content.clone();
                break;
            }
            if tool_calls_count >= version.max_tool_calls {
                let _ = self
                    .repos
                    .agents
                    .finish_run(
                        run_id,
                        "failed",
                        None,
                        Some("AGENT_MAX_STEPS".into()),
                        Some("max tool calls exceeded".into()),
                        total_cost,
                    )
                    .await;
                return Ok(AgentRunResult {
                    run_id: run_id.to_string(),
                    status: "failed".into(),
                    steps: step,
                    output: None,
                    tool_calls: tool_call_records,
                    error: Some("max tool calls exceeded".into()),
                    cost_microunits: total_cost,
                });
            }

            // 记录 assistant tool_calls 消息 + 逐个授权/执行
            messages.push(CanonicalMessage {
                role: MessageRole::Assistant,
                content: response.content.clone().unwrap_or_default(),
                tool_call_id: None,
                name: None,
            });
            for call in &response.tool_calls {
                tool_calls_count += 1;
                let authorized = tools.iter().any(|t| t.key == call.name);
                let _started = Instant::now();
                let mut record = ToolCall {
                    id: uuid::Uuid::new_v4().to_string(),
                    trace_id: Some(format!("agent-{run_id}")),
                    agent_run_id: Some(run_id.to_string()),
                    tool_id: tools
                        .iter()
                        .find(|t| t.key == call.name)
                        .map(|t| t.id.clone())
                        .unwrap_or_default(),
                    tool_key: call.name.clone(),
                    status: if authorized {
                        "running".into()
                    } else {
                        "denied".into()
                    },
                    arguments: serde_json::from_str(&call.arguments).unwrap_or(json!({})),
                    error_code: if authorized {
                        None
                    } else {
                        Some("AGENT_TOOL_DENIED".into())
                    },
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    latency_ms: None,
                };
                let result = if authorized {
                    let tool = tools.iter().find(|t| t.key == call.name).unwrap();
                    // Tool 超时（§20.4）
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(tool.timeout_ms.max(1) as u64),
                        self.tools.execute(tool, &record.arguments),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => ToolResult {
                            status: "timeout".into(),
                            content: String::new(),
                            structured_data: None,
                            error: Some("tool timeout".into()),
                            latency_ms: tool.timeout_ms,
                        },
                    }
                } else {
                    ToolResult {
                        status: "denied".into(),
                        content: String::new(),
                        structured_data: None,
                        error: Some("tool not in agent allowlist".into()),
                        latency_ms: 0,
                    }
                };
                record.status = result.status.clone();
                record.completed_at = Some(chrono::Utc::now());
                record.latency_ms = Some(result.latency_ms);
                self.repos.agents.insert_tool_call(record.clone()).await?;
                tool_call_records.push(record);

                messages.push(CanonicalMessage {
                    role: MessageRole::Tool,
                    content: match &result.error {
                        Some(e) => format!("error: {e}"),
                        None => result.content.clone(),
                    },
                    tool_call_id: Some(call.id.clone()),
                    name: Some(call.name.clone()),
                });
            }
        }

        let status = if step >= version.max_steps && final_output.is_none() {
            let _ = self
                .repos
                .agents
                .finish_run(
                    run_id,
                    "failed",
                    None,
                    Some("AGENT_MAX_STEPS".into()),
                    Some("max steps reached".into()),
                    total_cost,
                )
                .await;
            "failed"
        } else {
            self.repos
                .agents
                .finish_run(
                    run_id,
                    "completed",
                    final_output.clone().map(|o| json!({"text": o})),
                    None,
                    None,
                    total_cost,
                )
                .await?;
            "completed"
        };
        Ok(AgentRunResult {
            run_id: run_id.to_string(),
            status: status.to_string(),
            steps: step,
            output: final_output,
            tool_calls: tool_call_records,
            error: None,
            cost_microunits: total_cost,
        })
    }

    pub async fn run_detail(&self, run_id: &str) -> Result<(AgentRun, Vec<ToolCall>), DomainError> {
        let run = self.repos.agents.get_run(run_id).await?;
        let calls = self.repos.agents.tool_calls_for_run(run_id).await?;
        Ok((run, calls))
    }

    pub async fn runs(&self, agent_id: &str) -> Result<Vec<AgentRun>, DomainError> {
        self.repos.agents.list_runs(agent_id, 50).await
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
                resource_type: Some("agent".into()),
                resource_id: Some(resource_id.into()),
                decision: None,
                payload_ref: None,
                metadata,
                created_at: chrono::Utc::now(),
            })
            .await;
    }
}

fn server_transport_timeout(server: &McpServer) -> i64 {
    server
        .config
        .get("timeoutMs")
        .and_then(|v| v.as_i64())
        .unwrap_or(30_000)
}
