//! Gateway 错误码（方案 §27.2）与流水线错误。

use aihub_domain::error::{DomainError, ErrorCategory};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    RateLimited,
    QuotaExceeded,
    ModelNotFound,
    ModelNotAllowed,
    RouteNotFound,
    ProviderUnavailable,
    ProviderAuthFailed,
    ProviderRateLimited,
    ProviderTimeout,
    ProviderError,
    ContextTooLong,
    ContentFiltered,
    StreamInterrupted,
}

impl GatewayCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            GatewayCode::InvalidRequest => "AIH_INVALID_REQUEST",
            GatewayCode::Unauthorized => "AIH_UNAUTHORIZED",
            GatewayCode::Forbidden => "AIH_FORBIDDEN",
            GatewayCode::RateLimited => "AIH_RATE_LIMITED",
            GatewayCode::QuotaExceeded => "AIH_QUOTA_EXCEEDED",
            GatewayCode::ModelNotFound => "AIH_MODEL_NOT_FOUND",
            GatewayCode::ModelNotAllowed => "AIH_MODEL_NOT_ALLOWED",
            GatewayCode::RouteNotFound => "AIH_ROUTE_NOT_FOUND",
            GatewayCode::ProviderUnavailable => "AIH_PROVIDER_UNAVAILABLE",
            GatewayCode::ProviderAuthFailed => "AIH_PROVIDER_AUTH_FAILED",
            GatewayCode::ProviderRateLimited => "AIH_PROVIDER_RATE_LIMITED",
            GatewayCode::ProviderTimeout => "AIH_PROVIDER_TIMEOUT",
            GatewayCode::ProviderError => "AIH_PROVIDER_ERROR",
            GatewayCode::ContextTooLong => "AIH_CONTEXT_TOO_LONG",
            GatewayCode::ContentFiltered => "AIH_CONTENT_FILTERED",
            GatewayCode::StreamInterrupted => "AIH_STREAM_INTERRUPTED",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            GatewayCode::InvalidRequest | GatewayCode::ContextTooLong | GatewayCode::ContentFiltered => 400,
            GatewayCode::Unauthorized => 401,
            GatewayCode::Forbidden | GatewayCode::ModelNotAllowed => 403,
            GatewayCode::ModelNotFound | GatewayCode::RouteNotFound => 404,
            GatewayCode::RateLimited | GatewayCode::QuotaExceeded | GatewayCode::ProviderRateLimited => 429,
            GatewayCode::ProviderUnavailable | GatewayCode::ProviderAuthFailed | GatewayCode::ProviderError => 502,
            GatewayCode::ProviderTimeout => 504,
            GatewayCode::StreamInterrupted => 502,
        }
    }

    /// 统一 Provider 错误类别 → Gateway 错误码（方案 §10.6/§12）。
    pub fn from_provider_category(category: ErrorCategory) -> Self {
        match category {
            ErrorCategory::Authentication => GatewayCode::ProviderAuthFailed,
            ErrorCategory::PermissionDenied => GatewayCode::ProviderAuthFailed,
            ErrorCategory::RateLimited => GatewayCode::ProviderRateLimited,
            ErrorCategory::QuotaExceeded => GatewayCode::QuotaExceeded,
            ErrorCategory::InvalidRequest => GatewayCode::InvalidRequest,
            ErrorCategory::ModelNotFound => GatewayCode::ModelNotFound,
            ErrorCategory::ContextLengthExceeded => GatewayCode::ContextTooLong,
            ErrorCategory::Timeout => GatewayCode::ProviderTimeout,
            ErrorCategory::Connection => GatewayCode::ProviderUnavailable,
            ErrorCategory::Provider5xx => GatewayCode::ProviderError,
            ErrorCategory::MalformedResponse => GatewayCode::ProviderError,
            ErrorCategory::ContentFiltered => GatewayCode::ContentFiltered,
            ErrorCategory::Unknown => GatewayCode::ProviderError,
        }
    }
}

impl fmt::Display for GatewayCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct PipelineError {
    pub code: GatewayCode,
    pub message: String,
}

impl PipelineError {
    pub fn new(code: GatewayCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(GatewayCode::InvalidRequest, message)
    }

    pub fn from_provider_error(err: &aihub_provider_core::ProviderError) -> Self {
        Self::new(
            GatewayCode::from_provider_category(err.category),
            err.message.clone(),
        )
    }

    pub fn from_domain(err: &DomainError) -> Self {
        let code = match err.code {
            aihub_domain::DomainErrorCode::NotFound => GatewayCode::ModelNotFound,
            aihub_domain::DomainErrorCode::Duplicate => GatewayCode::InvalidRequest,
            aihub_domain::DomainErrorCode::ValidationFailed => GatewayCode::InvalidRequest,
            aihub_domain::DomainErrorCode::ResourceInUse => GatewayCode::InvalidRequest,
            aihub_domain::DomainErrorCode::Internal => GatewayCode::ProviderError,
        };
        Self::new(code, err.message.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_category_maps_to_gateway_codes() {
        assert_eq!(
            GatewayCode::from_provider_category(ErrorCategory::RateLimited).as_str(),
            "AIH_PROVIDER_RATE_LIMITED"
        );
        assert_eq!(
            GatewayCode::from_provider_category(ErrorCategory::Authentication).http_status(),
            502
        );
        assert_eq!(GatewayCode::RateLimited.http_status(), 429);
    }
}
