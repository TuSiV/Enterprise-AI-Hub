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

//! ObjectStorage Port（方案 §21.2）：本地文件系统 / S3-compatible。

use async_trait::async_trait;
use serde_json::Value;

use crate::error::DomainError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size_bytes: i64,
    #[serde(flatten)]
    pub extra: Value,
}

#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn put(&self, key: &str, body: Vec<u8>) -> Result<ObjectMeta, DomainError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, DomainError>;
    async fn delete(&self, key: &str) -> Result<(), DomainError>;
    async fn exists(&self, key: &str) -> Result<bool, DomainError>;
}
