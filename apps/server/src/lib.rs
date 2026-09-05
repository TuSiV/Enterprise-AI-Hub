//! Server Host（方案 §24）：装配 Rust Core（repos/services/pipeline），
//! 暴露 Admin API、OpenAI 兼容 Gateway、健康检查与静态 Web UI。
//! Desktop 模式（Tauri）复用同一 bootstrap。

pub mod admin;

use std::net::SocketAddr;
use std::sync::Arc;

use aihub_application::limiter::RateLimiter;
use aihub_application::pipeline::ChatPipeline;
use aihub_application::playground::PlaygroundService;
use aihub_application::registry::ProviderRegistry;
use aihub_application::resolver::ModelResolver;
use aihub_application::seed;
use aihub_application::services::{
    ApplicationService, ModelService, ProviderService, QueryService, VirtualModelService,
};
use aihub_application::Repos;
use aihub_config::Config;
use aihub_provider_core::ProviderFactory;
use aihub_provider_openai_compatible::OpenAICompatibleFactory;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct AppState {
    pub repos: Repos,
    pub config: Config,
    pub admin_token: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub providers: Arc<ProviderService>,
    pub models: Arc<ModelService>,
    pub virtual_models: Arc<VirtualModelService>,
    pub applications: Arc<ApplicationService>,
    pub queries: Arc<QueryService>,
    pub playground: Arc<PlaygroundService>,
    pub pipeline: Arc<ChatPipeline>,
    pub registry: Arc<ProviderRegistry>,
    pub resolver: Arc<ModelResolver>,
    pub limiter: Arc<RateLimiter>,
}

pub struct Core {
    pub state: AppState,
    pub router: Router,
    pub admin_token: String,
    pub gateway_endpoint: String,
}

fn generate_admin_token() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..40)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// 读取或生成 Admin Token；生成后写入 data dir（0600），供 Desktop 壳读取。
pub fn resolve_admin_token(config: &Config) -> anyhow::Result<String> {
    if let Some(token) = &config.auth.admin_token {
        return Ok(token.clone());
    }
    let path = config.data_dir().join("admin_token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    let token = generate_admin_token();
    std::fs::create_dir_all(config.data_dir())?;
    std::fs::write(&path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!(target: "aihub::core", path = %path.display(), "generated admin token");
    Ok(token)
}

pub async fn bootstrap(config: Config) -> anyhow::Result<Core> {
    aihub_telemetry::init(&config.telemetry.log_level);
    tracing::info!(target: "aihub::core", version = VERSION, mode = ?config.mode, "starting AI Hub core");

    // SQLite（Desktop 默认）；PostgreSQL Adapter 在 M10 按同一 Port 接入。
    let pool = aihub_persistence::open_sqlite(&config.db_path()).await?;

    let repos = Repos {
        providers: Arc::new(aihub_persistence::SqliteProviderRepository::new(pool.clone())),
        provider_health: Arc::new(aihub_persistence::SqliteProviderHealthRepository::new(pool.clone())),
        models: Arc::new(aihub_persistence::SqliteModelRepository::new(pool.clone())),
        virtual_models: Arc::new(aihub_persistence::SqliteVirtualModelRepository::new(pool.clone())),
        applications: Arc::new(aihub_persistence::SqliteApplicationRepository::new(pool.clone())),
        api_keys: Arc::new(aihub_persistence::SqliteApiKeyRepository::new(pool.clone())),
        quota: Arc::new(aihub_persistence::SqliteQuotaRepository::new(pool.clone())),
        requests: Arc::new(aihub_persistence::SqliteRequestRepository::new(pool.clone())),
        usage: Arc::new(aihub_persistence::SqliteUsageRepository::new(pool.clone())),
        audit: Arc::new(aihub_persistence::SqliteAuditRepository::new(pool.clone())),
    };

    // SecretStore（方案 §21.3）：Desktop 默认 OS Keychain；开发/CI 可选 memory；
    // Server 可选 env。macOS 上读取未签名二进制写入的钥匙串条目会触发授权弹窗，
    // 因此 backend 显式可配。
    let secrets: Arc<dyn aihub_secrets::SecretStore> = match config.auth.secret_backend.as_deref() {
        Some("memory") => Arc::new(aihub_secrets::MemorySecretStore::new()),
        Some("env") => Arc::new(aihub_secrets::CompositeSecretStore::new(vec![
            Box::new(aihub_secrets::EnvSecretStore::new()),
            Box::new(aihub_secrets::MemorySecretStore::new()),
        ])),
        _ => Arc::from(aihub_secrets::default_store()),
    };
    let factories: Vec<Arc<dyn ProviderFactory>> = vec![Arc::new(OpenAICompatibleFactory)];
    let registry = Arc::new(ProviderRegistry::new(factories, repos.providers.clone(), secrets.clone()));
    let resolver = Arc::new(ModelResolver::new(
        repos.virtual_models.clone(),
        repos.models.clone(),
        repos.providers.clone(),
    ));
    let limiter = Arc::new(RateLimiter::new());
    let breakers = Arc::new(aihub_application::CircuitBreakerRegistry::new());
    let pipeline = Arc::new(ChatPipeline::new(
        repos.clone(),
        registry.clone(),
        resolver.clone(),
        limiter.clone(),
        breakers.clone(),
    ));

    // 预置数据（§16.3 / playground 应用）
    seed::seed_defaults(&repos).await;

    let providers = Arc::new(ProviderService::new(repos.clone(), registry.clone(), secrets));
    let models = Arc::new(ModelService::new(repos.clone()));
    let virtual_models = Arc::new(VirtualModelService::new(repos.clone()));
    let applications = Arc::new(ApplicationService::new(repos.clone()));
    let queries = Arc::new(QueryService::new(repos.clone(), resolver.clone()));
    let playground = Arc::new(PlaygroundService::new(pipeline.clone(), repos.clone()));

    let admin_token = resolve_admin_token(&config)?;
    let gateway_endpoint = format!("http://{}:{}", config.gateway.host, config.gateway.port);

    let state = AppState {
        repos,
        config: config.clone(),
        admin_token: admin_token.clone(),
        started_at: chrono::Utc::now(),
        providers,
        models,
        virtual_models,
        applications,
        queries,
        playground,
        pipeline,
        registry,
        resolver,
        limiter,
    };

    let gateway_state = aihub_gateway::GatewayState {
        pipeline: state.pipeline.clone(),
        resolver: state.resolver.clone(),
        registry: state.registry.clone(),
        repos: state.repos.clone(),
    };

    let cors = CorsLayer::permissive();
    let base: Router<()> = Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .with_state(state.clone());
    let router = base
        .nest("/api", admin::router(state.clone()))
        .merge(aihub_gateway::router(gateway_state))
        .layer(cors);

    // 静态 Web UI（Desktop/Server 共用）
    let router = match config.web_dist_path() {
        Some(dist) => {
            let index = dist.join("index.html");
            let serve = ServeDir::new(&dist)
                .append_index_html_on_directories(true)
                .not_found_service(ServeFile::new(index));
            router.fallback_service(serve)
        }
        None => router.fallback(get(api_root)),
    };

    Ok(Core {
        state,
        router,
        admin_token,
        gateway_endpoint,
    })
}

async fn health_live() -> &'static str {
    "ok"
}

async fn health_ready(axum::extract::State(state): axum::extract::State<AppState>) -> axum::response::Response {
    // ready = DB 可读写
    let ok = state.repos.models.count_enabled().await.is_ok();
    if ok {
        (axum::http::StatusCode::OK, "ready").into_response()
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "db unavailable").into_response()
    }
}

async fn api_root() -> axum::response::Html<&'static str> {
    axum::response::Html(concat!(
        "<html><body style='font-family:sans-serif;padding:2rem'>",
        "<h2>Enterprise AI Hub</h2>",
        "<p>Web UI 构建产物不存在（web/dist）。请运行 <code>cd web && npm install && npm run build</code>。",
        "<br/>Admin API: <code>/api/v1/admin/...</code> · Gateway: <code>/v1/chat/completions</code></p>",
        "</body></html>"
    ))
}

/// 端口冲突自动退让（§23.3，desktop 模式）；server 模式直接报错。
pub async fn bind(config: &Config) -> anyhow::Result<(SocketAddr, u16)> {
    let host = config.gateway.host.clone();
    let base_port = config.gateway.port;
    let attempts = if config.mode == aihub_config::Mode::Desktop { 20 } else { 1 };
    for offset in 0..attempts {
        let port = base_port + offset;
        let addr: SocketAddr = format!("{host}:{port}").parse()?;
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                let _ = listener;
                return Ok((addr, port));
            }
            Err(e) if offset + 1 < attempts => {
                tracing::warn!(target: "aihub::core", port, error = %e, "port busy, trying next");
            }
            Err(e) => return Err(anyhow::anyhow!("bind {addr} failed: {e}")),
        }
    }
    unreachable!()
}
