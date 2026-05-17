#![allow(dead_code)]

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod fs;
pub mod models;

use axum::extract::DefaultBodyLimit;
use axum::extract::FromRef;
use axum::http::{header, Request};
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use config::AppConfig;
use fs::lock::LockManager;
use fs::watcher::FileWatcher;
use models::mount::MountPoint;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub mount_points: Vec<MountPoint>,
    pub lock_manager: LockManager,
    pub file_watcher: FileWatcher,
}

impl FromRef<AppState> for AppConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<AppState> for Vec<MountPoint> {
    fn from_ref(state: &AppState) -> Self {
        state.mount_points.clone()
    }
}

impl FromRef<AppState> for LockManager {
    fn from_ref(state: &AppState) -> Self {
        state.lock_manager.clone()
    }
}

impl FromRef<AppState> for FileWatcher {
    fn from_ref(state: &AppState) -> Self {
        state.file_watcher.clone()
    }
}

fn build_cors(config: &AppConfig) -> Option<CorsLayer> {
    if config.cors_origins.is_empty() {
        return None;
    }

    let is_permissive = config.cors_origins.len() == 1 && config.cors_origins[0] == "*";

    if is_permissive {
        Some(CorsLayer::permissive())
    } else {
        let origins: Vec<_> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();

        Some(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers(Any),
        )
    }
}

/// Generate a request ID from a random u64.
fn generate_request_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

/// Middleware that ensures every request has an X-Request-Id.
/// If the client provides one, it's used; otherwise a random hex ID is generated.
async fn request_id_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_request_id);

    let span = tracing::info_span!("request", request_id = %request_id);
    let _enter = span.enter();

    let mut req = req;
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        "X-Request-Id",
        header::HeaderValue::from_str(&request_id).unwrap(),
    );
    resp
}

/// Extension type holding the request ID for extractors.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .init();

    let config = AppConfig::from_env();
    let mount_points = MountPoint::parse_all(&config.mount_points_raw);

    tracing::info!(
        "Mount points: {:?}",
        mount_points.iter().map(|m| &m.name).collect::<Vec<_>>()
    );

    let listen_addr = config.listen_addr.clone();
    let cors = build_cors(&config);

    let state = AppState {
        config,
        mount_points,
        lock_manager: LockManager::new(),
        file_watcher: FileWatcher::new(),
    };

    let mut app = Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .merge(api::routes())
        .with_state(state)
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024)) // 1GB upload limit
        .layer(axum::middleware::from_fn(request_id_middleware))
        .layer(TraceLayer::new_for_http());

    if let Some(cors_layer) = cors {
        app = app.layer(cors_layer);
    }

    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .expect("Failed to bind");

    tracing::info!("Listening on {}", listen_addr);

    // Graceful shutdown on SIGINT/SIGTERM
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    tracing::info!("Server shut down");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}
