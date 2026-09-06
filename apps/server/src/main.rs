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

//! aihub-server：Desktop(本地)/Server 双模式宿主（方案 §23/§24）。
//! 子命令：serve（默认）/ backup / restore（方案 §28.2/§28.3）。

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aihub-server", version, about = "Enterprise AI Hub core host")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// 配置文件路径（TOML）
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// 运行模式：desktop | server
    #[arg(long, global = true)]
    mode: Option<String>,
    /// 覆盖网关端口
    #[arg(long, global = true)]
    port: Option<u16>,
    /// 启动后向 stderr 打印 admin token（本地开发便利）
    #[arg(long, default_value_t = false, global = true)]
    print_admin_token: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 启动服务（默认行为）
    Serve,
    /// 备份数据库到 tar 包（manifest.json + aihub.db）
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    /// 从备份包恢复数据库（目标库必须不存在）
    Restore {
        #[arg(long)]
        from: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mode_override = args.mode.as_deref().and_then(|m| match m {
        "desktop" => Some(aihub_config::Mode::Desktop),
        "server" => Some(aihub_config::Mode::Server),
        _ => None,
    });
    let mut config = aihub_config::Config::load(args.config.as_deref(), mode_override)?;
    if let Some(port) = args.port {
        config.gateway.port = port;
    }

    match args.command {
        Some(Command::Backup { output }) => {
            aihub_telemetry::init("info");
            let db_path = config.db_path();
            aihub_server::backup::create_backup(&db_path, &output).await?;
            eprintln!(
                "backup written: {} (db: {})",
                output.display(),
                db_path.display()
            );
            return Ok(());
        }
        Some(Command::Restore { from }) => {
            aihub_telemetry::init("info");
            let db_path = config.db_path();
            aihub_server::backup::restore_backup(&from, &db_path).await?;
            eprintln!("restored {} -> {}", from.display(), db_path.display());
            return Ok(());
        }
        Some(Command::Serve) | None => {}
    }

    // 绑定（desktop 模式端口冲突自动退让 §23.3）
    let (addr, actual_port) = aihub_server::bind(&config).await?;
    config.gateway.port = actual_port;

    let core = aihub_server::bootstrap(config).await?;
    let admin_token = core.admin_token.clone();

    eprintln!();
    eprintln!("  Enterprise AI Hub v{}", aihub_server::VERSION);
    eprintln!("  ├─ Admin API + Web UI : http://{}/", addr);
    eprintln!("  └─ OpenAI Gateway     : http://{}/v1", addr);
    if args.print_admin_token {
        eprintln!("  └─ Admin Token        : {admin_token}");
    }
    eprintln!();

    // 背景任务：Provider 健康巡检（§22.2 Runtime Status）
    {
        let providers = core.state.providers.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let Ok(list) = providers.list().await else {
                    continue;
                };
                for provider in list {
                    if provider.enabled {
                        let _ = providers.test(&provider.id).await;
                    }
                }
            }
        });
    }

    let router = core.router;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!(target: "aihub::core", "shutting down");
}
