//! HTTP host over the Loom command bus.
//!
//! Binds loopback by default. Studio and other clients talk JSON; semantics stay
//! in `wvx-command-bus` (not reimplemented here).
//!
//! SEC-001: session token + CORS allowlist + workspace path roots.

mod api;
mod security;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{HeaderValue, Method};
use axum::middleware;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use wvx_registry_client::LocalRegistry;

use crate::security::SecurityConfig;

/// Shared process state.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<LocalRegistry>,
    pub security: Arc<SecurityConfig>,
}

#[tokio::main]
async fn main() {
    // Gate F external demo (unpublished) — host-only, not part of wvx-command-bus.
    wvx_adapter_external_demo::register();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let listen: SocketAddr = std::env::var("WVX_HTTP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:43917".into())
        .parse()
        .unwrap_or_else(|e| {
            tracing::error!("invalid WVX_HTTP_ADDR: {e}");
            std::process::exit(1);
        });
    let security = match SecurityConfig::from_env().with_listen_addr(listen) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };
    let registry = match open_registry() {
        Ok(r) => Arc::new(r),
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };

    let state = AppState {
        registry: registry.clone(),
        security: security.clone(),
    };

    let token = Arc::new(security.session_token.clone());
    let cors = build_cors(&security.cors_origins);

    let app = api::router(state)
        .layer(middleware::from_fn_with_state(
            token,
            security::require_token,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = listen;

    tracing::info!(
        %addr,
        remote = security.remote_mode,
        registry = %registry.root().display(),
        "loom-server listening"
    );
    if security.remote_mode {
        tracing::info!("remote mode: X-WVX-Token required; auth bootstrap disabled");
    } else {
        tracing::info!(
            "session token (set header X-WVX-Token): {}",
            security.session_token
        );
    }
    tracing::info!("Studio (HTTP only): cd ../loom-studio && npm run dev → http://127.0.0.1:5173");
    tracing::info!("Alpha smoke: powershell -File ./scripts/alpha-smoke.ps1");
    tracing::info!(
        "workspace roots: {:?}",
        security
            .workspace_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
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

fn build_cors(origins: &[String]) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();
    if parsed.is_empty() {
        return CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(tower_http::cors::Any);
    }
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any)
}

fn open_registry() -> Result<LocalRegistry, String> {
    if let Ok(path) = std::env::var("WVX_REGISTRY") {
        return LocalRegistry::open(PathBuf::from(path)).map_err(|e| e.to_string());
    }
    LocalRegistry::open_default().map_err(|e| {
        format!("{e}\nSet WVX_REGISTRY or run from the weavatrix-loom repo root (registry-dev/).")
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
