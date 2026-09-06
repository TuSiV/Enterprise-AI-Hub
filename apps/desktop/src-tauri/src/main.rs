//! Desktop Host（方案 §23 / §5）：Tauri 只是 Desktop Adapter，
//! Rust Core 以库形态嵌入本进程；窗口加载本地 Core 的 Web UI。
//!
//! 启动顺序（§23.1）：数据目录 → SQLite → migrations → Core Services →
//! Local Gateway（loopback，端口冲突自动退让）→ 打开窗口。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

struct CoreHandle {
    admin_token: String,
    endpoint: String,
    shutdown: tokio::sync::watch::Sender<bool>,
}

#[tauri::command]
fn get_admin_token(state: tauri::State<CoreHandle>) -> String {
    state.admin_token.clone()
}

#[tauri::command]
fn get_endpoint(state: tauri::State<CoreHandle>) -> String {
    state.endpoint.clone()
}

/// 单实例（§23.2）：探测默认端口是否已有运行中的实例；
/// 是则把用户引导到已运行实例（打开系统浏览器）并退出第二个进程。
fn already_running(endpoint: &str) -> bool {
    let url = format!("{endpoint}/health/live");
    std::process::Command::new("curl")
        .args(["-sf", "-m", "2", &url])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    {
        let config = aihub_config::Config::load(None, Some(aihub_config::Mode::Desktop))
            .expect("load config");
        let endpoint = format!("http://127.0.0.1:{}", config.gateway.port);
        if already_running(&endpoint) {
            open_in_browser(&endpoint);
            eprintln!("Enterprise AI Hub is already running at {endpoint}");
            return;
        }
    }

    // Core 启动是同步语义（窗口需要 endpoint）；迁移失败时直接退出并记录日志（§23.1 Recovery 由日志与重试承载）。
    let (handle, endpoint) = runtime.block_on(async {
        let mut config = aihub_config::Config::load(None, Some(aihub_config::Mode::Desktop))
            .expect("load config");
        let (addr, port) = aihub_server::bind(&config).await.expect("bind gateway");
        config.gateway.port = port;
        let core = aihub_server::bootstrap(config)
            .await
            .expect("bootstrap core");
        let endpoint = format!("http://{}", addr);
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let server_router = core.router.clone();
        // axum Server 运行在后台，随 shutdown 信号退出
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr).await.expect("listen");
            let server = axum::serve(listener, server_router).with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            });
            let _ = server.await;
        });
        let handle = CoreHandle {
            admin_token: core.admin_token.clone(),
            endpoint: endpoint.clone(),
            shutdown: shutdown_tx,
        };
        (handle, endpoint)
    });

    tauri::Builder::default()
        .manage(handle)
        .setup(move |app| {
            let url: tauri::Url = endpoint.parse().expect("valid url");
            let window = tauri::webview::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External(url),
            )
            .title("Enterprise AI Hub")
            .inner_size(1280.0, 820.0)
            .build()?;
            let _ = window;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_admin_token, get_endpoint])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    if let Some(state) = window.app_handle().try_state::<CoreHandle>() {
                        let _ = state.shutdown.send(true);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
