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

//! Shared API types: OpenAI-compatible gateway wire types (方案 §11/§13.3)
//! and Admin API DTOs. Used by the gateway/application crates and mirrored
//! manually in `web/src/api/types.ts`.

pub mod admin;
pub mod common;
pub mod gateway;

pub use common::{ApiErrorBody, PageMeta};
