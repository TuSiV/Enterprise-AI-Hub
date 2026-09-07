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

//! backup/restore 契约测试（方案 §28.2）。

use aihub_persistence::open_sqlite;

#[tokio::test]
async fn backup_restore_roundtrip() {
    let dir = std::env::temp_dir().join(format!("aihub-bk-test-'{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("aihub.db");

    let pool = open_sqlite(&db).await.unwrap();
    sqlx::query("INSERT INTO providers (id, key, name, kind, base_url, credential_configured, timeout_ms, max_retries, enabled, status, health, config_json, created_at, updated_at) VALUES ('p1','k','n','openai_compatible','http://x/v1',0,1000,0,1,'active','unknown','{}','2026-01-01T00:00:00.000000Z','2026-01-01T00:00:00.000000Z')")
        .execute(&pool)
        .await
        .unwrap();
    // 模拟 WAL 存在（打开即 WAL）；不关闭池直接备份
    let archive = dir.join("backup.tar");
    aihub_server::backup::create_backup(&db, &archive)
        .await
        .expect("create backup while pool open");

    let restored_dir = dir.join("restored");
    std::fs::create_dir_all(&restored_dir).unwrap();
    let restored_db = restored_dir.join("aihub.db");
    aihub_server::backup::restore_backup(&archive, &restored_db)
        .await
        .expect("restore");

    let pool2 = open_sqlite(&restored_db).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM providers")
        .fetch_one(&pool2)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // 目标已存在时 restore 必须拒绝
    let err = aihub_server::backup::restore_backup(&archive, &restored_db).await;
    assert!(err.is_err());
    let _ = pool.close().await;
    let _ = pool2.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

fn append(builder: &mut tar::Builder<std::fs::File>, name: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    builder.append_data(&mut header, name, bytes).unwrap();
}

#[tokio::test]
async fn restore_rejects_links_missing_manifest_duplicates_and_invalid_database() {
    for case in [
        "symlink",
        "hardlink",
        "missing",
        "duplicate",
        "foreign",
        "invalid-db",
        "empty-db",
        "unexpected",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bad.tar");
        let destination = dir.path().join("restored.db");
        let mut builder = tar::Builder::new(std::fs::File::create(&archive).unwrap());
        if case == "symlink" || case == "hardlink" {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(if case == "symlink" {
                tar::EntryType::Symlink
            } else {
                tar::EntryType::Link
            });
            header.set_size(0);
            header.set_mode(0o600);
            builder
                .append_link(&mut header, "aihub.db", "/etc/passwd")
                .unwrap();
        } else {
            append(
                &mut builder,
                "aihub.db",
                if case == "empty-db" {
                    b""
                } else {
                    b"not a SQLite database"
                },
            );
        }
        if case == "duplicate" {
            append(&mut builder, "aihub.db", b"duplicate");
        }
        if case == "unexpected" {
            append(&mut builder, "other.txt", b"unexpected");
        }
        if case != "missing" {
            let manifest = serde_json::json!({
                "app": if case == "foreign" { "other-app" } else { "enterprise-ai-hub" },
                "schema_note": "", "created_at": "2026-09-07T00:00:00Z", "secret_included": false
            });
            append(
                &mut builder,
                "manifest.json",
                manifest.to_string().as_bytes(),
            );
        }
        builder.finish().unwrap();
        drop(builder);
        let result = aihub_server::backup::restore_backup(&archive, &destination).await;
        assert!(result.is_err(), "{case} must fail");
        assert!(!destination.exists(), "{case} must not publish a database");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "temporary files must be removed: {case}"
        );
    }
}
