//! Domain 层（方案 §7.1 / §41.1）：不依赖 sqlx/axum/reqwest/具体 Provider。
//! 基础设施通过 repos.rs 中定义的 Port 由 persistence 等 Adapter 实现。

pub mod canonical;
pub mod cost;
pub mod entities;
pub mod error;
pub mod platform;
pub mod prompt;
pub mod repos;

pub use canonical::{
    CanonicalChatRequest, CanonicalChatResponse, CanonicalEmbeddingRequest,
    CanonicalEmbeddingResponse, CanonicalMessage, CanonicalUsage, DiscoveredModel, MessageRole,
    ResponseFormat, StreamEvent, ToolChoice, ToolDefinitionData, UsageSource,
};
pub use cost::{CostBreakdown, Pricing};
pub use entities::*;
pub use error::{DomainError, DomainErrorCode, DomainResource, ErrorCategory};
