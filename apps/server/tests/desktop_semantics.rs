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

//! Desktop 语义验证（§31.6）：端口冲突自动退让（§23.3）、单实例语义、备份/恢复已单测。

use aihub_config::{Config, GatewayConfig, Mode};
use std::net::SocketAddr;

fn desktop_config(port: u16, data_dir: &std::path::Path) -> Config {
    Config {
        mode: Mode::Desktop,
        data_dir: Some(data_dir.to_string_lossy().to_string()),
        gateway: GatewayConfig {
            host: "127.0.0.1".into(),
            port,
            request_timeout_ms: 10_000,
        },
        ..Default::default()
    }
}

/// §23.3：desktop 模式端口被占用时自动选择下一个端口；server 模式直接失败。
#[tokio::test]
async fn port_conflict_falls_back_in_desktop_mode() {
    let dir = std::env::temp_dir().join(format!("aihub-port-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    // 占住端口
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();

    let config = desktop_config(port, &dir);
    let (addr, actual) = aihub_server::bind(&config).await.unwrap();
    assert_ne!(actual, port, "desktop mode must fall back to next port");
    assert!(actual > port);
    let _: SocketAddr = addr;
}

/// server 模式端口被占用必须报错（不允许静默漂移端口）。
#[tokio::test]
async fn port_conflict_fails_in_server_mode() {
    let dir = std::env::temp_dir().join(format!("aihub-port-srv-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();

    let mut config = desktop_config(port, &dir);
    config.mode = Mode::Server;
    assert!(aihub_server::bind(&config).await.is_err());
}

/// HTTP GET 探测（desktop 壳 already_running 的核心逻辑）。
async fn http_probe(url: &str) -> bool {
    match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(client) => matches!(
            client.get(url).send().await,
            Ok(response) if response.status().as_u16() == 200
        ),
        Err(_) => false,
    }
}

/// 单实例（§23.2）：第二个 desktop 实例启动时能探测到已运行实例（health/live 响应）。
#[tokio::test]
async fn single_instance_probe_contract() {
    async fn live() -> &'static str {
        "ok"
    }
    let app = axum::Router::new().route("/health/live", axum::routing::get(live));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        http_probe(&format!("http://{addr}/health/live")).await,
        "probe must detect running instance"
    );

    // 无实例端口 → false
    let idle = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let idle_port = idle.local_addr().unwrap().port();
    assert!(
        !http_probe(&format!("http://127.0.0.1:{idle_port}/health/live")).await,
        "probe must not false-positive on idle port"
    );
}
