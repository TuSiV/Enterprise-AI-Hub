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

//! Playground 执行入口（§18.3）：走同一条 Gateway 流水线，使用内置 playground 应用。

use serde_json::{json, Value};

use aihub_api_types::admin::PlaygroundRunRequest;
use aihub_domain::canonical::{CanonicalChatRequest, CanonicalMessage, MessageRole};
use aihub_domain::error::DomainError;
use aihub_domain::DomainResource;

use crate::error::PipelineError;
use crate::pipeline::{AuthContext, ChatExecution, PipelineStreamEvent, StreamExecution};
use crate::{ChatPipeline, Repos, PLAYGROUND_APPLICATION_KEY};

pub struct PlaygroundService {
    pipeline: std::sync::Arc<ChatPipeline>,
    repos: Repos,
}

fn content_to_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn build_canonical(request: &PlaygroundRunRequest) -> Result<CanonicalChatRequest, PipelineError> {
    let mut messages = Vec::new();
    if let Some(system) = &request.system {
        if !system.is_empty() {
            messages.push(CanonicalMessage {
                role: MessageRole::System,
                content: system.clone(),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            });
        }
    }
    for message in &request.messages {
        let role = message
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user");
        let content = message
            .get("content")
            .map(content_to_text)
            .unwrap_or_default();
        messages.push(CanonicalMessage {
            role: MessageRole::parse(role),
            content,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        });
    }
    if messages.is_empty() {
        return Err(PipelineError::invalid_request(
            "at least one message is required",
        ));
    }
    Ok(CanonicalChatRequest {
        model: request.model.clone(),
        messages,
        tools: Vec::new(),
        tool_choice: None,
        temperature: request.temperature,
        top_p: request.top_p,
        max_output_tokens: request.max_tokens,
        response_format: None,
        stream: request.stream.unwrap_or(false),
        metadata: json!({"origin": "playground"}),
    })
}

impl PlaygroundService {
    pub fn new(pipeline: std::sync::Arc<ChatPipeline>, repos: Repos) -> Self {
        Self { pipeline, repos }
    }

    async fn playground_ctx(&self) -> Result<AuthContext, DomainError> {
        let application = match self
            .repos
            .applications
            .get_by_key(PLAYGROUND_APPLICATION_KEY)
            .await
        {
            Ok(app) => app,
            Err(_) => {
                return Err(DomainError::not_found(
                    DomainResource::Application,
                    PLAYGROUND_APPLICATION_KEY,
                ))
            }
        };
        Ok(AuthContext {
            application,
            api_key_id: None,
            actor_type: "admin",
        })
    }

    pub async fn run(&self, request: PlaygroundRunRequest) -> Result<ChatExecution, PipelineError> {
        let ctx = self
            .playground_ctx()
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        let canonical = build_canonical(&request)?;
        self.pipeline.execute_chat(&ctx, canonical).await
    }

    pub async fn run_stream(
        &self,
        request: PlaygroundRunRequest,
    ) -> Result<StreamExecution, PipelineError> {
        let ctx = self
            .playground_ctx()
            .await
            .map_err(|e| PipelineError::from_domain(&e))?;
        let canonical = build_canonical(&request)?;
        self.pipeline.execute_chat_stream(&ctx, canonical).await
    }

    /// 供 UI 展示流式事件（转发 PipelineStreamEvent，不做协议转换）。
    pub fn map_stream_event(event: PipelineStreamEvent) -> PipelineStreamEvent {
        event
    }
}
