//! Shared API types: OpenAI-compatible gateway wire types (方案 §11/§13.3)
//! and Admin API DTOs. Used by the gateway/application crates and mirrored
//! manually in `web/src/api/types.ts`.

pub mod admin;
pub mod common;
pub mod gateway;

pub use common::{ApiErrorBody, PageMeta};
