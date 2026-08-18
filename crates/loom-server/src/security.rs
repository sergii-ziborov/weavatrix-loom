//! Local server hardening (SEC-001 / M0 + P2 remote).
//!
//! - Session token required for mutating/data routes (not `/health`)
//! - Tokens are CSPRNG (`getrandom`); remote bind requires `WVX_SESSION_TOKEN`
//! - `/api/v1/auth/bootstrap` is loopback-only and disabled in remote mode
//! - CORS allowlist (env `WVX_CORS_ORIGINS`, default Studio localhost)
//! - Forge/export paths restricted to approved workspace roots

use std::net::SocketAddr;
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
    /// True when `WVX_SESSION_TOKEN` was supplied (required for remote mode).
    pub token_from_env: bool,
    /// Non-loopback bind or `WVX_REMOTE=1`.
    pub remote_mode: bool,
    pub cors_origins: Vec<String>,
    pub workspace_roots: Vec<PathBuf>,
}

impl SecurityConfig {
    pub fn from_env() -> Self {
        let (session_token, token_from_env) = match std::env::var("WVX_SESSION_TOKEN") {
            Ok(t) if !t.trim().is_empty() => (t, true),
            _ => (csprng_token(), false),
        };
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
            token_from_env,
            remote_mode: env_truthy("WVX_REMOTE"),
            cors_origins,
            workspace_roots,
        }
    }

    /// Apply the listen address. Remote bind without `WVX_SESSION_TOKEN` fails closed.
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Result<Self, String> {
        if env_truthy("WVX_REMOTE") || !addr.ip().is_loopback() {
            self.remote_mode = true;
        }
        if self.remote_mode && !self.token_from_env {
            return Err(
                "remote mode requires WVX_SESSION_TOKEN (CSPRNG auto-token is loopback-only)"
                    .into(),
            );
        }
        Ok(self)
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

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// 256-bit CSPRNG session token (`wvx_` + 64 hex chars).
pub fn csprng_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("CSPRNG unavailable");
    let mut hex = String::with_capacity(4 + 64);
    hex.push_str("wvx_");
    for b in buf {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

fn tokens_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
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
        Some(t) if tokens_eq(&t, token.as_str()) => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok": false,
                "diagnostics": [
                    "missing or invalid session token",
                    "loopback: GET /api/v1/auth/bootstrap; remote: set WVX_SESSION_TOKEN and X-WVX-Token"
                ]
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn csprng_token_is_256_bit_hex() {
        let a = csprng_token();
        let b = csprng_token();
        assert!(a.starts_with("wvx_"), "{a}");
        assert_eq!(a.len(), 4 + 64);
        assert_ne!(a, b);
        assert!(a[4..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn remote_bind_requires_env_token() {
        let cfg = SecurityConfig {
            session_token: "auto".into(),
            token_from_env: false,
            remote_mode: false,
            cors_origins: vec![],
            workspace_roots: vec![],
        };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 43917);
        assert!(cfg.with_listen_addr(addr).is_err());
    }

    #[test]
    fn remote_bind_ok_with_env_token() {
        let cfg = SecurityConfig {
            session_token: "secret".into(),
            token_from_env: true,
            remote_mode: false,
            cors_origins: vec![],
            workspace_roots: vec![],
        };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 43917);
        let cfg = cfg.with_listen_addr(addr).unwrap();
        assert!(cfg.remote_mode);
    }

    #[test]
    fn loopback_allows_generated_token() {
        if env_truthy("WVX_REMOTE") {
            return;
        }
        let cfg = SecurityConfig {
            session_token: "auto".into(),
            token_from_env: false,
            remote_mode: false,
            cors_origins: vec![],
            workspace_roots: vec![],
        };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 43917);
        let cfg = cfg.with_listen_addr(addr).unwrap();
        assert!(!cfg.remote_mode);
    }
}
