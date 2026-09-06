//! Desktop / Server 数据备份与恢复（方案 §28.2/§28.3）。
//! 备份包：manifest.json + aihub.db（SQLite VACUUM INTO 生成一致快照）。Secret 默认不进入备份包。

use std::path::Path;

use serde::Serialize;
use sqlx::sqlite::SqlitePoolOptions;

#[derive(Debug, Serialize)]
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
    let workdir = std::env::temp_dir().join(format!("aihub-backup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir)?;
    let snapshot = workdir.join("aihub.db");

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
    sqlx::query(&format!("VACUUM INTO '{}'", snapshot.display()))
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

    std::fs::remove_dir_all(&workdir)?;
    Ok(())
}

pub async fn restore_backup(archive: &Path, db_path: &Path) -> anyhow::Result<()> {
    if db_path.exists() {
        anyhow::bail!(
            "target database {} already exists; remove it (or move it aside) before restore",
            db_path.display()
        );
    }
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let workdir = std::env::temp_dir().join(format!("aihub-restore-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir)?;
    let tarball = std::fs::File::open(archive)?;
    tar::Archive::new(tarball).unpack(&workdir)?;
    let restored = workdir.join("aihub.db");
    if !restored.exists() {
        std::fs::remove_dir_all(&workdir)?;
        anyhow::bail!("archive does not contain aihub.db");
    }
    std::fs::copy(&restored, db_path)?;
    std::fs::remove_dir_all(&workdir)?;
    Ok(())
}
