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

//! Domain 层（方案 §7.1 / §41.1）：不依赖 sqlx/axum/reqwest/具体 Provider。
//! 基础设施通过 repos.rs 中定义的 Port 由 persistence 等 Adapter 实现。

pub mod canonical;
pub mod cost;
pub mod entities;
pub mod error;
pub mod platform;
pub mod pricing;
pub mod provider_presets;
pub mod prompt;
pub mod repos;
pub mod storage;

pub use canonical::{
    CanonicalChatRequest, CanonicalChatResponse, CanonicalEmbeddingRequest,
    CanonicalEmbeddingResponse, CanonicalMessage, CanonicalUsage, DiscoveredModel, MessageRole,
    ResponseFormat, StreamEvent, ToolChoice, ToolDefinitionData, UsageSource,
};
pub use cost::{CostBreakdown, Pricing};
pub use entities::*;
pub use error::{DomainError, DomainErrorCode, DomainResource, ErrorCategory};
pub use storage::{ObjectMeta, ObjectStorage};
