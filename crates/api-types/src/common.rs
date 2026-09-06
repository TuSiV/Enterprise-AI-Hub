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

//! 统一响应格式（方案 §13.3）：Admin API 的 data/meta/error 信封与分页。

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

impl ApiErrorBody {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorDetail {
                code: code.to_string(),
                message: message.into(),
                request_id: None,
                details: Value::Null,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageMeta {
    pub page: u64,
    #[serde(rename = "pageSize")]
    pub page_size: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataResponse<T: Serialize> {
    pub data: T,
    pub meta: Value,
}

impl<T: Serialize> DataResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            meta: serde_json::json!({}),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub data: Vec<T>,
    pub meta: PageMeta,
}
