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

//! Desktop / Server 数据备份与恢复（方案 §28.2/§28.3）。
//! 备份包：manifest.json + aihub.db（SQLite VACUUM INTO 生成一致快照）。Secret 默认不进入备份包。

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePoolOptions;
use std::io::{Read, Seek, Write};

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupManifest {
    pub app: String,
    pub schema_note: String,
    pub created_at: String,
    pub secret_included: bool,
}

pub async fn create_backup(db_path: &Path, output: &Path) -> anyhow::Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let workdir = tempfile::Builder::new().prefix("aihub-backup-").tempdir()?;
    let snapshot = workdir.path().join("aihub.db");

    // VACUUM INTO 产生一致快照，不阻塞运行中的实例。
    // 与运行实例保持 WAL 模式一致，避免打开遗留 -wal/-shm 的库时 SQLITE_CANTOPEN。
    use std::str::FromStr;
    let options =
        sqlx::sqlite::SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(false)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::query("VACUUM INTO ?")
        .bind(snapshot.to_string_lossy().as_ref())
        .execute(&pool)
        .await?;
    pool.close().await;

    let manifest = BackupManifest {
        app: "enterprise-ai-hub".into(),
        schema_note: "contains aihub.db only; secrets stay in SecretStore".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        secret_included: false,
    };
    let tarball = std::fs::File::create(output)?;
    let mut builder = tar::Builder::new(tarball);
    builder.append_path_with_name(&snapshot, "aihub.db")?;
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_json.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, "manifest.json", manifest_json.as_slice())?;
    builder.finish()?;

    Ok(())
}

// Bound untrusted archives before allocating memory or writing database bytes.
const MAX_DATABASE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

pub async fn restore_backup(archive: &Path, db_path: &Path) -> anyhow::Result<()> {
    if db_path.symlink_metadata().is_ok() {
        anyhow::bail!("target database {} already exists", db_path.display());
    }
    let parent = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    // Same filesystem for atomic, no-clobber publication after validation.
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    let tarball = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(tarball);
    let mut seen_database = false;
    let mut manifest = None;
    for entry in tar.entries()? {
        let mut entry = entry?;
        anyhow::ensure!(
            entry.header().entry_type().is_file(),
            "backup entries must be regular files"
        );
        let name = entry.path_bytes().into_owned();
        match name.as_slice() {
            b"aihub.db" if !seen_database => {
                anyhow::ensure!(
                    entry.size() <= MAX_DATABASE_BYTES,
                    "backup database exceeds 8 GiB"
                );
                std::io::copy(&mut entry, staged.as_file_mut())?;
                seen_database = true;
            }
            b"manifest.json" if manifest.is_none() => {
                anyhow::ensure!(
                    entry.size() <= MAX_MANIFEST_BYTES,
                    "backup manifest exceeds 64 KiB"
                );
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                let value: BackupManifest = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    value.app == "enterprise-ai-hub" && !value.secret_included,
                    "unsupported backup manifest"
                );
                chrono::DateTime::parse_from_rfc3339(&value.created_at)?;
                manifest = Some(value);
            }
            _ => anyhow::bail!("unexpected or duplicate backup entry"),
        }
    }
    anyhow::ensure!(
        seen_database && manifest.is_some(),
        "backup requires aihub.db and manifest.json"
    );
    staged.as_file_mut().flush()?;
    staged.as_file().sync_all()?;
    staged.as_file_mut().rewind()?;
    let mut signature = [0u8; 16];
    staged.as_file_mut().read_exact(&mut signature)?;
    anyhow::ensure!(
        &signature == b"SQLite format 3\0",
        "backup is not a SQLite database"
    );
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(staged.path())
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let checks = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_all(&pool)
        .await;
    pool.close().await;
    anyhow::ensure!(checks? == ["ok"], "backup database integrity check failed");
    staged.persist_noclobber(db_path)?;
    Ok(())
}
