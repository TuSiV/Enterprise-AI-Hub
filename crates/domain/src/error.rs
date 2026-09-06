//! 统一错误模型：Domain Error 枚举（§41.1）与 Provider 错误分类（§10.6）。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Authentication,
    PermissionDenied,
    RateLimited,
    QuotaExceeded,
    InvalidRequest,
    ModelNotFound,
    ContextLengthExceeded,
    Timeout,
    Connection,
    Provider5xx,
    MalformedResponse,
    ContentFiltered,
    Unknown,
}

impl ErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::Authentication => "authentication",
            ErrorCategory::PermissionDenied => "permission_denied",
            ErrorCategory::RateLimited => "rate_limited",
            ErrorCategory::QuotaExceeded => "quota_exceeded",
            ErrorCategory::InvalidRequest => "invalid_request",
            ErrorCategory::ModelNotFound => "model_not_found",
            ErrorCategory::ContextLengthExceeded => "context_length_exceeded",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::Connection => "connection",
            ErrorCategory::Provider5xx => "provider_5xx",
            ErrorCategory::MalformedResponse => "malformed_response",
            ErrorCategory::ContentFiltered => "content_filtered",
            ErrorCategory::Unknown => "unknown",
        }
    }

    /// Gateway retry/failover 基于统一错误类别（方案 §12.2/§12.3）。
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            ErrorCategory::Connection
                | ErrorCategory::Timeout
                | ErrorCategory::RateLimited
                | ErrorCategory::Provider5xx
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainResource {
    Provider,
    Model,
    VirtualModel,
    Application,
    ApiKey,
    Request,
    KnowledgeBase,
    Document,
    Prompt,
    Agent,
    Tool,
    Eval,
    User,
    Policy,
}

impl DomainResource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DomainResource::Provider => "provider",
            DomainResource::Model => "model",
            DomainResource::VirtualModel => "virtual_model",
            DomainResource::Application => "application",
            DomainResource::ApiKey => "api_key",
            DomainResource::Request => "request",
            DomainResource::KnowledgeBase => "knowledge_base",
            DomainResource::Document => "document",
            DomainResource::Prompt => "prompt",
            DomainResource::Agent => "agent",
            DomainResource::Tool => "tool",
            DomainResource::Eval => "eval",
            DomainResource::User => "user",
            DomainResource::Policy => "policy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainErrorCode {
    NotFound,
    Duplicate,
    ValidationFailed,
    ResourceInUse,
    Internal,
}

impl DomainErrorCode {
    /// 错误码规范（方案 §27）
    pub fn as_str(&self) -> &'static str {
        match self {
            DomainErrorCode::NotFound => "NOT_FOUND",
            DomainErrorCode::Duplicate => "DUPLICATE",
            DomainErrorCode::ValidationFailed => "VALIDATION_FAILED",
            DomainErrorCode::ResourceInUse => "RESOURCE_IN_USE",
            DomainErrorCode::Internal => "INTERNAL",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{code:?} {resource:?}: {message}")]
pub struct DomainError {
    pub code: DomainErrorCode,
    pub resource: DomainResource,
    pub message: String,
    pub resource_id: Option<String>,
}

impl DomainError {
    pub fn not_found(resource: DomainResource, id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            code: DomainErrorCode::NotFound,
            resource,
            message: format!("{} not found: {id}", resource.as_str()),
            resource_id: Some(id),
        }
    }

    pub fn duplicate(resource: DomainResource, key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            code: DomainErrorCode::Duplicate,
            resource,
            message: format!("{} duplicate key: {key}", resource.as_str()),
            resource_id: None,
        }
    }

    pub fn validation(resource: DomainResource, message: impl Into<String>) -> Self {
        Self {
            code: DomainErrorCode::ValidationFailed,
            resource,
            message: message.into(),
            resource_id: None,
        }
    }

    pub fn in_use(
        resource: DomainResource,
        id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: DomainErrorCode::ResourceInUse,
            resource,
            message: message.into(),
            resource_id: Some(id.into()),
        }
    }

    pub fn internal(resource: DomainResource, message: impl Into<String>) -> Self {
        Self {
            code: DomainErrorCode::Internal,
            resource,
            message: message.into(),
            resource_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_categories_match_plan() {
        assert!(ErrorCategory::Connection.retryable());
        assert!(ErrorCategory::Timeout.retryable());
        assert!(ErrorCategory::RateLimited.retryable());
        assert!(ErrorCategory::Provider5xx.retryable());
        assert!(!ErrorCategory::Authentication.retryable());
        assert!(!ErrorCategory::InvalidRequest.retryable());
        assert!(!ErrorCategory::ContextLengthExceeded.retryable());
        assert!(!ErrorCategory::ContentFiltered.retryable());
    }
}
