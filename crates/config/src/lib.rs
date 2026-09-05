//! 配置加载（方案 §26）：默认值 < config.toml < 环境变量（AIHUB_*）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid config: {0}")]
    Invalid(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Desktop,
    Server,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout_ms: u64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8787,
            request_timeout_ms: 120_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub driver: String,
    pub url: Option<String>,
    pub url_env: Option<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            driver: "sqlite".to_string(),
            url: None,
            url_env: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub driver: String,
    pub path: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            driver: "local".to_string(),
            path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub mode: String,
    pub enabled: bool,
    pub url: Option<String>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            mode: "managed".to_string(),
            enabled: false,
            url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub log_level: String,
    pub otel_enabled: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            otel_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct AuthConfig {
    /// 固定 admin token；未设置时首次启动生成并写入 data dir 的 admin_token 文件。
    pub admin_token: Option<String>,
    pub trusted_header_user: bool,
    /// SecretStore 后端：keyring（默认，OS 钥匙串）| memory | env。
    /// 开发/CI 环境建议 memory，避免 macOS 钥匙串授权弹窗。
    pub secret_backend: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct WebConfig {
    pub dist_path: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub mode: Mode,
    pub data_dir: Option<String>,
    pub gateway: GatewayConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub vector: StorageConfig,
    pub runtime: RuntimeConfig,
    pub telemetry: TelemetryConfig,
    pub auth: AuthConfig,
    pub web: WebConfig,
}

impl Config {
    /// CLI 参数 > 环境变量 > 配置文件 > 默认值。
    pub fn load(config_path: Option<&Path>, mode_override: Option<Mode>) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        let file_path = config_path
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::var("AIHUB_CONFIG").ok().map(PathBuf::from));

        if let Some(path) = file_path {
            if path.exists() {
                let raw = std::fs::read_to_string(&path)?;
                let file_cfg: Config = toml::from_str(&raw)?;
                config.merge_from(file_cfg);
                tracing::info!(target: "aihub::config", path = %path.display(), "loaded config file");
            } else {
                tracing::warn!(target: "aihub::config", path = %path.display(), "config file not found, using defaults");
            }
        }

        config.apply_env();
        if let Some(mode) = mode_override {
            config.mode = mode;
        }
        Ok(config)
    }

    fn merge_from(&mut self, other: Config) {
        if other.data_dir.is_some() {
            self.data_dir = other.data_dir;
        }
        self.gateway = other.gateway;
        self.database = other.database;
        self.storage = other.storage;
        self.vector = other.vector;
        self.runtime = other.runtime;
        self.telemetry = other.telemetry;
        self.auth = other.auth;
        self.web = other.web;
        self.mode = other.mode;
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("AIHUB_MODE") {
            if v == "desktop" || v == "server" {
                self.mode = if v == "desktop" { Mode::Desktop } else { Mode::Server };
            }
        }
        if let Ok(v) = std::env::var("AIHUB_DATABASE_URL") {
            self.database.url = Some(v);
        }
        if let Ok(v) = std::env::var("AIHUB_GATEWAY_HOST") {
            self.gateway.host = v;
        }
        if let Ok(v) = std::env::var("AIHUB_GATEWAY_PORT") {
            if let Ok(port) = v.parse() {
                self.gateway.port = port;
            }
        }
        if let Ok(v) = std::env::var("AIHUB_RUNTIME_URL") {
            self.runtime.url = Some(v);
            self.runtime.enabled = true;
        }
        if let Ok(v) = std::env::var("AIHUB_LOG_LEVEL") {
            self.telemetry.log_level = v;
        }
        if let Ok(v) = std::env::var("AIHUB_ADMIN_TOKEN") {
            self.auth.admin_token = Some(v);
        }
        if let Ok(v) = std::env::var("AIHUB_SECRET_BACKEND") {
            self.auth.secret_backend = Some(v);
        }
        if let Ok(v) = std::env::var("AIHUB_DATA_DIR") {
            self.data_dir = Some(v);
        }
    }

    /// 本地数据目录（方案附录 E）：桌面模式使用系统 Application Support，
    /// 不在代码中硬编码用户名路径；server 模式默认 ./data。
    pub fn data_dir(&self) -> PathBuf {
        if let Some(dir) = &self.data_dir {
            return PathBuf::from(dir);
        }
        match self.mode {
            Mode::Desktop => dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Enterprise AI Hub"),
            Mode::Server => PathBuf::from("./data"),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        if let Some(url) = &self.database.url {
            if self.database.driver == "sqlite" {
                return PathBuf::from(url);
            }
        }
        if let Some(env_key) = &self.database.url_env {
            if let Ok(url) = std::env::var(env_key) {
                return PathBuf::from(url);
            }
        }
        self.data_dir().join("aihub.db")
    }

    pub fn documents_dir(&self) -> PathBuf {
        self.storage
            .path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("documents"))
    }

    pub fn web_dist_path(&self) -> Option<PathBuf> {
        if let Some(p) = &self.web.dist_path {
            let path = PathBuf::from(p);
            if path.exists() {
                return Some(path);
            }
            return None;
        }
        // 常见相对位置：仓库内运行或发布目录
        for candidate in [
            PathBuf::from("../web/dist"),
            PathBuf::from("./web/dist"),
            self.data_dir().join("web"),
        ] {
            if candidate.join("index.html").exists() {
                return Some(candidate);
            }
        }
        None
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}
