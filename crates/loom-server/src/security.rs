//! Local server hardening (SEC-001 / M0).
//!
//! - Session token required for mutating/data routes (not `/health`)
//! - CORS allowlist (env `WVX_CORS_ORIGINS`, default Studio localhost)
//! - Forge/export paths restricted to approved workspace roots

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Security / workspace configuration for loom-server.
#[derive(Clone)]
pub struct SecurityConfig {
    pub session_token: String,
    pub cors_origins: Vec<String>,
    pub workspace_roots: Vec<PathBuf>,
}

impl SecurityConfig {
    pub fn from_env() -> Self {
        let session_token = std::env::var("WVX_SESSION_TOKEN").unwrap_or_else(|_| {
            // Stable-enough random for a process lifetime
            format!("wvx-{}", random_token())
        });
        let cors_origins = std::env::var("WVX_CORS_ORIGINS")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_else(|_| {
                vec![
                    "http://localhost:5173".into(),
                    "http://127.0.0.1:5173".into(),
                    "http://[::1]:5173".into(),
                ]
            });
        let workspace_roots = std::env::var("WVX_WORKSPACE_ROOTS")
            .map(|s| {
                s.split(';')
                    .chain(s.split(','))
                    .map(|x| x.trim())
                    .filter(|x| !x.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_else(|_| {
                let mut roots = Vec::new();
                if let Ok(cwd) = std::env::current_dir() {
                    roots.push(cwd);
                }
                if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
                    roots.push(PathBuf::from(&home).join("Documents").join("GitHub"));
                    roots.push(PathBuf::from(home));
                }
                roots
            });
        Self {
            session_token,
            cors_origins,
            workspace_roots,
        }
    }

    pub fn path_allowed(&self, path: &Path) -> bool {
        let Ok(canon) = path.canonicalize().or_else(|_| {
            // Allow non-existing targets if parent is under a root
            if let Some(parent) = path.parent() {
                parent
                    .canonicalize()
                    .map(|p| p.join(path.file_name().unwrap_or_default()))
            } else {
                Err(std::io::Error::other("no parent"))
            }
        }) else {
            return false;
        };
        for root in &self.workspace_roots {
            let Ok(root_c) = root.canonicalize() else {
                continue;
            };
            if canon.starts_with(&root_c) {
                return true;
            }
        }
        false
    }
}

fn random_token() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    std::process::id().hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Axum middleware: require `X-WVX-Token` (or `Authorization: Bearer`) except health/protocol/bootstrap.
pub async fn require_token(
    State(token): State<Arc<String>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();
    if path == "/health" || path == "/api/v1/protocol" || path == "/api/v1/auth/bootstrap" {
        return next.run(req).await;
    }
    let header_tok = req
        .headers()
        .get("x-wvx-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer ").map(|x| x.to_string()))
        });
    match header_tok {
        Some(t) if t == token.as_str() => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok": false,
                "diagnostics": [
                    "missing or invalid session token",
                    "GET /api/v1/auth/bootstrap from loopback or set WVX_SESSION_TOKEN / X-WVX-Token"
                ]
            })),
        )
            .into_response(),
    }
}
