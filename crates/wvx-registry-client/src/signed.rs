//! Verifiable promotion reports (integrity-bound; not a hosted PKI).
//!
//! A report is accepted only when:
//! - `payload_digest` matches a recomputed SHA-256 of the canonical payload;
//! - HMAC-SHA256 (`signature`) verifies with the configured key;
//! - implementation / profile ids match the subject.
//!
//! Without `WVX_PROMOTION_HMAC_KEY`, signed reports are **rejected** (fail-closed).
//! Live collection inside `promote` is the default path.

use crate::evidence_artifact::{sha256_hex, CaseResult};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use wvx_ir::AxisFact;

pub const SIGNED_REPORTS_SCHEMA: &str = "wvx.promotion_reports.v0.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedPromotionReports {
    pub schema_version: String,
    pub implementation_id: String,
    pub profile_id: String,
    pub runner_identity: String,
    pub recorded_at_unix: u64,
    pub case_results: Vec<CaseResult>,
    pub build: AxisFact,
    pub bench: AxisFact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bench_fingerprint: Option<String>,
    pub license: AxisFact,
    pub security: AxisFact,
    pub payload_digest: String,
    /// HMAC-SHA256 hex of `payload_digest` with `WVX_PROMOTION_HMAC_KEY`.
    pub signature: String,
}

impl SignedPromotionReports {
    pub fn canonical_payload(&self) -> String {
        serde_json::json!({
            "schema_version": self.schema_version,
            "implementation_id": self.implementation_id,
            "profile_id": self.profile_id,
            "runner_identity": self.runner_identity,
            "recorded_at_unix": self.recorded_at_unix,
            "case_results": self.case_results,
            "build": self.build,
            "bench": self.bench,
            "bench_fingerprint": self.bench_fingerprint,
            "license": self.license,
            "security": self.security,
        })
        .to_string()
    }
}

/// HMAC-SHA256 (RFC 2104) using SHA-256 block size 64.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut k = if key.len() > BLOCK {
        Sha256::digest(key).to_vec()
    } else {
        key.to_vec()
    };
    k.resize(BLOCK, 0);
    let mut ipad = vec![0x36u8; BLOCK];
    let mut opad = vec![0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(msg);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner_hash);
    let hash = outer.finalize();
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

pub fn promotion_hmac_key() -> Option<Vec<u8>> {
    let raw = std::env::var("WVX_PROMOTION_HMAC_KEY").ok()?;
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    Some(t.as_bytes().to_vec())
}

pub fn sign_reports(mut reports: SignedPromotionReports, key: &[u8]) -> SignedPromotionReports {
    reports.schema_version = SIGNED_REPORTS_SCHEMA.into();
    let payload = reports.canonical_payload();
    reports.payload_digest = sha256_hex(payload.as_bytes());
    reports.signature = hmac_sha256(key, reports.payload_digest.as_bytes());
    reports
}

pub fn verify_signed_reports(
    reports: &SignedPromotionReports,
    expected_impl: &str,
    expected_profile: &str,
) -> Result<(), RegistryError> {
    if reports.schema_version != SIGNED_REPORTS_SCHEMA {
        return Err(err(format!(
            "signed reports schema `{}` != `{SIGNED_REPORTS_SCHEMA}`",
            reports.schema_version
        )));
    }
    if reports.implementation_id != expected_impl {
        return Err(err(format!(
            "signed reports impl `{}` != `{expected_impl}`",
            reports.implementation_id
        )));
    }
    if reports.profile_id != expected_profile {
        return Err(err(format!(
            "signed reports profile `{}` != `{expected_profile}`",
            reports.profile_id
        )));
    }
    if reports.runner_identity.trim().is_empty() {
        return Err(err("signed reports missing runner_identity"));
    }
    let recomputed = sha256_hex(reports.canonical_payload().as_bytes());
    if recomputed != reports.payload_digest {
        return Err(err(format!(
            "signed reports payload_digest mismatch: {} vs recomputed {recomputed}",
            reports.payload_digest
        )));
    }
    let Some(key) = promotion_hmac_key() else {
        return Err(err(
            "signed reports rejected: WVX_PROMOTION_HMAC_KEY is not set (fail-closed)",
        ));
    };
    let expect_sig = hmac_sha256(&key, reports.payload_digest.as_bytes());
    if expect_sig != reports.signature {
        return Err(err("signed reports HMAC signature mismatch"));
    }
    if reports.case_results.is_empty() {
        return Err(err("signed reports have no case_results"));
    }
    if reports.case_results.iter().any(|c| !c.ok) {
        return Err(err("signed reports contain failing cases"));
    }
    let _ = Path::new(".");
    Ok(())
}

fn err(msg: impl Into<String>) -> RegistryError {
    RegistryError::Parse(Path::new("<signed-reports>").to_path_buf(), msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_roundtrip() {
        let mac = hmac_sha256(b"key", b"msg");
        assert!(mac.starts_with("sha256:"));
        assert_eq!(mac, hmac_sha256(b"key", b"msg"));
        assert_ne!(mac, hmac_sha256(b"key", b"other"));
    }
}
