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

//! S3ObjectStorage 集成测试（M10，§31.7 S3 adapter）：
//! 对真实 MinIO 验证 SigV4 PUT/GET/DELETE/exists。
//! 运行（需 MinIO 与桶）：
//!   MINIO_ROOT_USER/PASSWORD=minio/... minio server /tmp/minio-data
//!   mc mb local/aihub
//!   TEST_S3_ENDPOINT=http://127.0.0.1:9000 AIHUB_S3_BUCKET=aihub \
//!     AWS_ACCESS_KEY_ID=minio AWS_SECRET_ACCESS_KEY=... \
//!     cargo test -p aihub-application --features s3-test --test s3_storage

#![cfg(feature = "s3-test")]

use aihub_application::s3_storage::S3ObjectStorage;
use aihub_domain::storage::ObjectStorage;

fn s3_from_env() -> S3ObjectStorage {
    std::env::set_var(
        "AIHUB_S3_ENDPOINT",
        std::env::var("TEST_S3_ENDPOINT").unwrap(),
    );
    std::env::set_var(
        "AIHUB_S3_BUCKET",
        std::env::var("AIHUB_S3_BUCKET").unwrap_or("aihub".into()),
    );
    std::env::set_var("AWS_REGION", "us-east-1");
    S3ObjectStorage::from_env().unwrap()
}

#[tokio::test]
async fn s3_put_get_delete_roundtrip() {
    let storage = s3_from_env();
    let key = format!("contract/{}.txt", uuid::Uuid::new_v4());

    storage
        .put(&key, b"hello minio".to_vec())
        .await
        .expect("put");
    assert!(storage.exists(&key).await.unwrap());

    let body = storage.get(&key).await.expect("get");
    assert_eq!(body, b"hello minio");

    storage.delete(&key).await.expect("delete");
    assert!(!storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn s3_get_missing_is_error() {
    let storage = s3_from_env();
    assert!(storage.get("contract/definitely-missing").await.is_err());
}
