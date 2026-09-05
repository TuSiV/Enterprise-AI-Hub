//! aihub-server：Desktop(本地)/Server 双模式宿主（方案 §23/§24）。

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aihub-server", version, about = "Enterprise AI Hub core host")]
struct Args {
    /// 配置文件路径（TOML）
    #[arg(long)]
    config: Option<PathBuf>,
    /// 运行模式：desktop | server
    #[arg(long)]
    mode: Option<String>,
    /// 覆盖网关端口
    #[arg(long)]
    port: Option<u16>,
    /// 启动后向 stderr 打印 admin token（本地开发便利）
    #[arg(long, default_value_t = false)]
    print_admin_token: bool,
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
                let Ok(list) = providers.list().await else { continue };
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
