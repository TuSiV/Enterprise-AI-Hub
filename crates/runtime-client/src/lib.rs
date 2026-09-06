//! Python Runtime 客户端与托管 Sidecar（M11，方案 §13.2/§23.4/§37）：
//! 版本握手、parse/embedding 等任务调用、进程托管（--port 0 握手 + 有界重启）。

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use tokio::sync::{watch, Mutex, RwLock};

pub const PROTOCOL_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeHealth {
    #[serde(rename = "runtimeVersion", default)]
    pub runtime_version: String,
    #[serde(rename = "protocolVersion", default)]
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParsedDocument {
    #[serde(default)]
    pub text: String,
    /// page_no 从 1 开始
    #[serde(default)]
    pub pages: Vec<ParsedPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedPage {
    pub page: i32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeState {
    Disabled,
    Starting,
    Running { endpoint: String },
    Failed { reason: String },
}

/// 托管 supervisor：拉起 python runtime，解析握手行，崩溃时有界重启（§23.4 不允许 crash loop）。
pub struct RuntimeSupervisor {
    state: RwLock<RuntimeState>,
    session_token: String,
    python_path: String,
    runtime_dir: PathBuf,
    restart_tx: watch::Sender<()>,
}

const MAX_RESTARTS: u32 = 5;

impl RuntimeSupervisor {
    pub fn disabled() -> Arc<Self> {
        let (tx, _) = watch::channel(());
        Arc::new(Self {
            state: RwLock::new(RuntimeState::Disabled),
            session_token: String::new(),
            python_path: String::new(),
            runtime_dir: PathBuf::new(),
            restart_tx: tx,
        })
    }

    pub fn new_managed(python_path: String, runtime_dir: PathBuf) -> Arc<Self> {
        let (tx, _) = watch::channel(());
        Arc::new(Self {
            state: RwLock::new(RuntimeState::Starting),
            session_token: format!("rt_{}", uuid::Uuid::new_v4().simple()),
            python_path,
            runtime_dir,
            restart_tx: tx,
        })
    }

    pub async fn state(&self) -> RuntimeState {
        self.state.read().await.clone()
    }

    async fn set_state(&self, state: RuntimeState) {
        *self.state.write().await = state;
    }

    /// 启动并托管进程；失败/崩溃按指数退避重启，超阈值进入 Failed（需手动重启）。
    pub fn spawn(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut attempt = 0u32;
            loop {
                *this.state.write().await = RuntimeState::Starting;
                match run_one(&this).await {
                    Ok(_endpoint) => {
                        attempt = 0;
                        // run_one 返回即进程退出（Running 状态已在握手后设置）
                        tracing::warn!(target: "aihub::runtime", "runtime process exited");
                    }
                    Err(e) => {
                        tracing::warn!(target: "aihub::runtime", error = %e, attempt, "runtime start failed");
                    }
                }
                attempt += 1;
                if attempt > MAX_RESTARTS {
                    *this.state.write().await = RuntimeState::Failed {
                        reason: format!("crash loop: exceeded {MAX_RESTARTS} restarts"),
                    };
                    return;
                }
                let backoff = Duration::from_millis(1000 * 2u64.pow(attempt.min(4)));
                let _ = &this.restart_tx;
                tokio::time::sleep(backoff).await;
            }
        });
    }

    /// 手动重启（UI 触发，Failed 状态后）。
    pub fn restart(self: Arc<Self>) {
        let _ = self.restart_tx.send(());
        Self::spawn(&self);
    }
}

/// 拉起一次 runtime 进程：等待 stdout 的 PORT=<n> 握手行，然后健康检查直到进程退出。
async fn run_one(supervisor: &RuntimeSupervisor) -> anyhow::Result<String> {
    let mut child = tokio::process::Command::new(&supervisor.python_path)
        .arg("-m")
        .arg("app.main")
        .arg("--port")
        .arg("0")
        .arg("--session-token")
        .arg(&supervisor.session_token)
        .current_dir(&supervisor.runtime_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut endpoint: Option<String> = None;
    // 握手：最多等 15 秒读 PORT= 行（§23.4）
    let _ = tokio::time::timeout(Duration::from_secs(15), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(port) = line.strip_prefix("PORT=") {
                endpoint = Some(format!("http://127.0.0.1:{port}"));
                break;
            }
        }
    })
    .await;
    let Some(endpoint) = endpoint else {
        let _ = child.kill().await;
        anyhow::bail!("runtime did not produce PORT handshake within 15s");
    };

    // 健康检查确认（含 token）
    let client = RuntimeClient::new(endpoint.clone(), supervisor.session_token.clone());
    let mut healthy = false;
    for _ in 0..10 {
        if client.health().await.is_ok() {
            healthy = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    if !healthy {
        let _ = child.kill().await;
        anyhow::bail!("runtime health check failed after handshake");
    }
    supervisor.set_state(RuntimeState::Running {
        endpoint: endpoint.clone(),
    })
    .await;
    tracing::info!(target: "aihub::runtime", %endpoint, "runtime sidecar ready");

    // 等待进程退出（托管期）
    let status = child.wait().await?;
    tracing::info!(target: "aihub::runtime", ?status, "runtime exited");
    Ok(endpoint)
}

/// Runtime HTTP 客户端（§13.2 Runtime API，Session Token 鉴权）。
#[derive(Clone)]
pub struct RuntimeClient {
    base_url: String,
    session_token: String,
    http: reqwest::Client,
}

impl RuntimeClient {
    pub fn new(base_url: String, session_token: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            session_token,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
        }
    }

    fn auth(&self) -> reqwest::header::HeaderValue {
        reqwest::header::HeaderValue::from_str(&self.session_token)
            .unwrap_or(reqwest::header::HeaderValue::from_static(""))
    }

    pub async fn health(&self) -> Result<RuntimeHealth, String> {
        let response = self
            .http
            .get(format!("{}/internal/v1/health", self.base_url))
            .header("x-aih-session-token", self.auth())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("runtime health status {}", response.status()));
        }
        let health: RuntimeHealth = response.json().await.map_err(|e| e.to_string())?;
        if health.protocol_version != PROTOCOL_VERSION {
            return Err(format!(
                "runtime protocol {} incompatible with core protocol {PROTOCOL_VERSION}（§37：拒绝启用高级功能）",
                health.protocol_version
            ));
        }
        Ok(health)
    }

    pub async fn parse_document(&self, filename: &str, mime_type: &str, bytes: Vec<u8>) -> Result<ParsedDocument, String> {
        use base64::Engine;
        let payload = serde_json::json!({
            "filename": filename,
            "mimeType": mime_type,
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(bytes),
        });
        let response = self
            .http
            .post(format!("{}/internal/v1/parse", self.base_url))
            .header("x-aih-session-token", self.auth())
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("runtime parse failed ({status}): {body}"));
        }
        response.json::<ParsedDocument>().await.map_err(|e| e.to_string())
    }
}

/// 独立模式客户端（Server 部署的 remote runtime，§13.2）。
pub fn remote_client(url: &str, session_token: &str) -> RuntimeClient {
    RuntimeClient::new(url.to_string(), session_token.to_string())
}

#[allow(dead_code)]
type SharedState = Arc<Mutex<()>>;
