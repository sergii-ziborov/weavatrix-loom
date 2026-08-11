//! HTTP host over the Loom command bus.
//!
//! Binds loopback by default. Studio and other clients talk JSON; semantics stay
//! in `wvx-command-bus` (not reimplemented here).

mod api;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use wvx_registry_client::LocalRegistry;

/// Shared process state.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<LocalRegistry>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let registry = match open_registry() {
        Ok(r) => Arc::new(r),
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };

    let state = AppState { registry: registry.clone() };
    let app = api::router(state).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    ).layer(TraceLayer::new_for_http());

    let addr: SocketAddr = std::env::var("WVX_HTTP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:43917".into())
        .parse()
        .unwrap_or_else(|e| {
            tracing::error!("invalid WVX_HTTP_ADDR: {e}");
            std::process::exit(1);
        });

    tracing::info!(
        %addr,
        registry = %registry.root().display(),
        "loom-server listening"
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        tracing::error!("bind {addr}: {e}");
        std::process::exit(1);
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| {
            tracing::error!("server error: {e}");
            std::process::exit(1);
        });
}

fn open_registry() -> Result<LocalRegistry, String> {
    if let Ok(path) = std::env::var("WVX_REGISTRY") {
        return LocalRegistry::open(PathBuf::from(path)).map_err(|e| e.to_string());
    }
    LocalRegistry::open_default().map_err(|e| {
        format!(
            "{e}\nSet WVX_REGISTRY or run from the weavatrix-loom repo root (registry-dev/)."
        )
    })
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
    tracing::info!("shutdown signal received");
}
