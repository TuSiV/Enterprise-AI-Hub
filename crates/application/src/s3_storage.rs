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

//! S3-compatible ObjectStorage Adapter（M10，方案 §21.2）：
//! AWS SigV4 签名的最小实现（PUT/GET/DELETE/HEAD，path-style，兼容 MinIO）。
//! 凭据经环境变量：AIHUB_S3_ENDPOINT / AIHUB_S3_BUCKET / AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_REGION。

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};

use aihub_domain::error::DomainError;
use aihub_domain::storage::{ObjectMeta, ObjectStorage};
use aihub_domain::DomainResource;

type HmacSha256 = Hmac<Sha256>;

pub struct S3ObjectStorage {
    http: reqwest::Client,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    region: String,
}

impl S3ObjectStorage {
    pub fn from_env() -> Result<Self, DomainError> {
        let endpoint = std::env::var("AIHUB_S3_ENDPOINT").map_err(|_| {
            DomainError::validation(
                DomainResource::Document,
                "AIHUB_S3_ENDPOINT is required for s3 storage driver",
            )
        })?;
        let bucket = std::env::var("AIHUB_S3_BUCKET").unwrap_or_else(|_| "aihub".into());
        let access_key = std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default();
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_default();
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".into());
        Ok(Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            bucket,
            access_key,
            secret_key,
            region,
        })
    }

    fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn sha256_hex(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    /// 生成 SigV4 Authorization 头（path-style：/{bucket}/{key}）。
    fn sign_request(
        &self,
        method: &str,
        path: &str,
        payload_hash: &str,
        amz_date: &str,
        date_stamp: &str,
    ) -> String {
        let host = self
            .endpoint
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .to_string();
        let canonical_request = format!(
            "{method}\n{path}\n\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload_hash}"
        );
        let scope = format!("{date_stamp}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            Self::sha256_hex(canonical_request.as_bytes())
        );
        let k_date = Self::hmac(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date_stamp.as_bytes(),
        );
        let k_region = Self::hmac(&k_date, self.region.as_bytes());
        let k_service = Self::hmac(&k_region, b"s3");
        let k_signing = Self::hmac(&k_service, b"aws4_request");
        let signature = hex::encode(Self::hmac(&k_signing, string_to_sign.as_bytes()));
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}",
            self.access_key
        )
    }

    fn request_builder(
        &self,
        method: reqwest::Method,
        key: &str,
        body: Option<Vec<u8>>,
    ) -> reqwest::RequestBuilder {
        let url = format!("{}/{}/{}", self.endpoint, self.bucket, key);
        let payload_hash = Self::sha256_hex(body.as_deref().unwrap_or(b""));
        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let path = format!("/{}/{}", self.bucket, key);
        let authorization = self.sign_request(
            method.as_str(),
            &path,
            &payload_hash,
            &amz_date,
            &date_stamp,
        );
        let mut builder = self
            .http
            .request(method, &url)
            .header("x-amz-content-sha256", &payload_hash)
            .header("x-amz-date", &amz_date)
            .header("Authorization", authorization);
        if let Some(body) = body {
            builder = builder.body(body);
        }
        builder
    }

    fn err(resource_op: &str, e: impl std::fmt::Display) -> DomainError {
        DomainError::internal(
            DomainResource::Document,
            format!("s3 {resource_op} failed: {e}"),
        )
    }
}

#[async_trait]
impl ObjectStorage for S3ObjectStorage {
    async fn put(&self, key: &str, body: Vec<u8>) -> Result<ObjectMeta, DomainError> {
        let size = body.len() as i64;
        let response = self
            .request_builder(reqwest::Method::PUT, key, Some(body))
            .send()
            .await
            .map_err(|e| Self::err("put", e))?;
        if !response.status().is_success() {
            return Err(Self::err("put", format!("status {}", response.status())));
        }
        Ok(ObjectMeta {
            key: key.to_string(),
            size_bytes: size,
            extra: json!({"backend": "s3", "bucket": self.bucket}),
        })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, DomainError> {
        let response = self
            .request_builder(reqwest::Method::GET, key, None)
            .send()
            .await
            .map_err(|e| Self::err("get", e))?;
        if !response.status().is_success() {
            return Err(Self::err("get", format!("status {}", response.status())));
        }
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| Self::err("get", e))
    }

    async fn delete(&self, key: &str) -> Result<(), DomainError> {
        let response = self
            .request_builder(reqwest::Method::DELETE, key, None)
            .send()
            .await
            .map_err(|e| Self::err("delete", e))?;
        if !response.status().is_success() {
            return Err(Self::err("delete", format!("status {}", response.status())));
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, DomainError> {
        let response = self
            .request_builder(reqwest::Method::HEAD, key, None)
            .send()
            .await
            .map_err(|e| Self::err("head", e))?;
        Ok(response.status().is_success())
    }
}
