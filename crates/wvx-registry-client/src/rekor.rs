//! Local Rekor **hashedrekord** v0.0.1 (not public Rekor, not Fulcio).
//!
//! Emits the same JSON kind Rekor uses (`hashedrekord` / `0.0.1`) and can
//! attach `tlogEntries` to an offline Sigstore bundle. Signatures remain
//! HMAC-SHA256 (`WVX_PROMOTION_HMAC_KEY`).
//!
//! Fail-closed:
//! - a Fulcio `certificate` in the bundle is rejected;
//! - `WVX_REKOR_URL` is refused (HMAC is not a Fulcio identity; we do not
//!   upload to `rekor.sigstore.dev` or any remote log).

use crate::attestation::TransparencyEntry;
use crate::evidence_artifact::sha256_hex;
use crate::signed::{hmac_sha256, promotion_hmac_key};
use crate::sigstore::{
    b64_decode, b64_encode, RekorInclusionPromise, RekorKindVersion, RekorLogId, RekorTlogEntry,
    SigstoreBundle,
};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const HASHEDREKORD_KIND: &str = "hashedrekord";
pub const HASHEDREKORD_VERSION: &str = "0.0.1";
pub const LOCAL_LOG_ID: &str = "wvx.local.rekor.v0";
pub const HMAC_KEYID_PEM: &str =
    "-----BEGIN WVX HMAC KEYID-----\nwvx-hmac\n-----END WVX HMAC KEYID-----\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashedRekordHash {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashedRekordData {
    pub hash: HashedRekordHash,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashedRekordPublicKey {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashedRekordSignature {
    pub content: String,
    #[serde(rename = "publicKey")]
    pub public_key: HashedRekordPublicKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashedRekordSpec {
    pub data: HashedRekordData,
    pub signature: HashedRekordSignature,
}

/// Rekor `hashedrekord` v0.0.1 proposed entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashedRekord {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub spec: HashedRekordSpec,
}

pub fn refuse_remote_rekor_url(url: Option<&str>) -> Result<(), RegistryError> {
    match url.map(str::trim).filter(|s| !s.is_empty()) {
        Some(url) => Err(err(format!(
            "refusing remote Rekor at `{url}`: HMAC hashedrekord is not a Fulcio identity"
        ))),
        None => Ok(()),
    }
}

pub fn refuse_remote_rekor() -> Result<(), RegistryError> {
    refuse_remote_rekor_url(std::env::var("WVX_REKOR_URL").ok().as_deref())
}

pub fn local_log_key_id() -> String {
    sha256_hex(LOCAL_LOG_ID.as_bytes())
        .trim_start_matches("sha256:")
        .to_string()
}

/// hashedrekord over `sha256_hex` (no `sha256:` prefix) + HMAC of that hex.
pub fn hashedrekord_for_digest(sha256_hex_value: &str, key: &[u8]) -> HashedRekord {
    let digest = sha256_hex_value.trim_start_matches("sha256:").to_string();
    let sig = hmac_sha256(key, digest.as_bytes());
    HashedRekord {
        api_version: HASHEDREKORD_VERSION.into(),
        kind: HASHEDREKORD_KIND.into(),
        spec: HashedRekordSpec {
            data: HashedRekordData {
                hash: HashedRekordHash {
                    algorithm: "sha256".into(),
                    value: digest,
                },
            },
            signature: HashedRekordSignature {
                content: b64_encode(sig.as_bytes()),
                public_key: HashedRekordPublicKey {
                    content: b64_encode(HMAC_KEYID_PEM.as_bytes()),
                },
            },
        },
    }
}

pub fn verify_hashedrekord(
    record: &HashedRekord,
    expected_sha256: &str,
) -> Result<(), RegistryError> {
    refuse_remote_rekor()?;
    if record.kind != HASHEDREKORD_KIND {
        return Err(err(format!(
            "rekor kind `{}` != `{HASHEDREKORD_KIND}`",
            record.kind
        )));
    }
    if record.api_version != HASHEDREKORD_VERSION {
        return Err(err(format!(
            "rekor apiVersion `{}` != `{HASHEDREKORD_VERSION}`",
            record.api_version
        )));
    }
    if record.spec.data.hash.algorithm != "sha256" {
        return Err(err("rekor hash algorithm must be sha256"));
    }
    let expect = expected_sha256.trim_start_matches("sha256:");
    if record.spec.data.hash.value != expect {
        return Err(err(format!(
            "rekor hash `{}` != `{expect}`",
            record.spec.data.hash.value
        )));
    }
    let pem = b64_decode(&record.spec.signature.public_key.content)
        .ok_or_else(|| err("rekor publicKey is not base64"))?;
    if pem != HMAC_KEYID_PEM.as_bytes() {
        return Err(err(
            "rekor publicKey is not the offline HMAC keyid (Fulcio certs are not accepted)",
        ));
    }
    let Some(key) = promotion_hmac_key() else {
        return Err(err(
            "rekor rejected: WVX_PROMOTION_HMAC_KEY is not set (fail-closed)",
        ));
    };
    let expect_sig = hmac_sha256(&key, expect.as_bytes());
    let got = b64_decode(&record.spec.signature.content)
        .ok_or_else(|| err("rekor signature is not base64"))?;
    if got != expect_sig.as_bytes() {
        return Err(err("rekor HMAC mismatch"));
    }
    Ok(())
}

pub fn inclusion_set(log_index: u64, body_b64: &str, integrated_time: u64, key: &[u8]) -> String {
    let msg = format!("{log_index}|{body_b64}|{integrated_time}");
    b64_encode(hmac_sha256(key, msg.as_bytes()).as_bytes())
}

/// Attach a local hashedrekord tlog entry to an offline bundle.
pub fn attach_local_tlog(
    bundle: &mut SigstoreBundle,
    log: &TransparencyEntry,
    record: &HashedRekord,
    key: &[u8],
) -> Result<(), RegistryError> {
    refuse_remote_rekor()?;
    if bundle.verification_material.certificate.is_some() {
        return Err(err(
            "Fulcio certificate present; Loom does not verify Fulcio identity",
        ));
    }
    let body = serde_json::to_vec(record).map_err(|e| err(format!("hashedrekord JSON: {e}")))?;
    let body_b64 = b64_encode(&body);
    let set = inclusion_set(log.seq, &body_b64, log.recorded_at_unix, key);
    bundle.verification_material.tlog_entries = vec![RekorTlogEntry {
        log_index: log.seq.to_string(),
        log_id: RekorLogId {
            key_id: local_log_key_id(),
        },
        kind_version: RekorKindVersion {
            kind: HASHEDREKORD_KIND.into(),
            version: HASHEDREKORD_VERSION.into(),
        },
        integrated_time: log.recorded_at_unix.to_string(),
        canonicalized_body: body_b64,
        inclusion_promise: RekorInclusionPromise {
            signed_entry_timestamp: set,
        },
    }];
    Ok(())
}

pub fn verify_tlog_entries(
    bundle: &SigstoreBundle,
    log: &[TransparencyEntry],
    expected_sha256: &str,
) -> Result<(), RegistryError> {
    refuse_remote_rekor()?;
    if bundle.verification_material.tlog_entries.is_empty() {
        return Err(err("bundle has no tlogEntries"));
    }
    let Some(key) = promotion_hmac_key() else {
        return Err(err(
            "rekor rejected: WVX_PROMOTION_HMAC_KEY is not set (fail-closed)",
        ));
    };
    for tlog in &bundle.verification_material.tlog_entries {
        if tlog.kind_version.kind != HASHEDREKORD_KIND
            || tlog.kind_version.version != HASHEDREKORD_VERSION
        {
            return Err(err("tlogEntries kindVersion is not hashedrekord/0.0.1"));
        }
        if tlog.log_id.key_id != local_log_key_id() {
            return Err(err("tlogEntries logId is not the local wvx log"));
        }
        let idx: u64 = tlog
            .log_index
            .parse()
            .map_err(|_| err("tlogEntries logIndex is not an integer"))?;
        let time: u64 = tlog
            .integrated_time
            .parse()
            .map_err(|_| err("tlogEntries integratedTime is not an integer"))?;
        let expect_set = inclusion_set(idx, &tlog.canonicalized_body, time, &key);
        if expect_set != tlog.inclusion_promise.signed_entry_timestamp {
            return Err(err("tlogEntries inclusionPromise HMAC mismatch"));
        }
        let body = b64_decode(&tlog.canonicalized_body)
            .ok_or_else(|| err("tlogEntries canonicalizedBody is not base64"))?;
        let record: HashedRekord = serde_json::from_slice(&body)
            .map_err(|e| err(format!("tlogEntries hashedrekord JSON: {e}")))?;
        verify_hashedrekord(&record, expected_sha256)?;
        let Some(entry) = log.iter().find(|e| e.seq == idx) else {
            return Err(err(format!(
                "tlogEntries logIndex {idx} is not in the local transparency log"
            )));
        };
        if entry.kind != crate::attestation::TransparencyKind::Rekor {
            return Err(err(format!("transparency seq {idx} is not a rekor entry")));
        }
    }
    Ok(())
}

fn err(msg: impl Into<String>) -> RegistryError {
    RegistryError::Parse(Path::new("<rekor>").to_path_buf(), msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestation::{append_transparency, TransparencyKind};
    use crate::sigstore::{wrap_attestation, SigstoreCertificate};
    use crate::SignedAttestation;

    fn sample_att() -> SignedAttestation {
        SignedAttestation {
            schema_version: crate::ATTESTATION_SCHEMA.into(),
            implementation_id: "demo.parse@1".into(),
            profile_id: "json-rfc8259-core-v1".into(),
            artifact_digest: "sha256:abc".into(),
            source_digest: "sha256:src".into(),
            sbom_digest: "sha256:sbom".into(),
            recorded_at_unix: 1,
            payload_digest: "sha256:deadbeef".into(),
            signature: "sig".into(),
        }
    }

    #[test]
    fn hashedrekord_roundtrip() {
        std::env::set_var("WVX_PROMOTION_HMAC_KEY", "rekor-test-key");
        let rec = hashedrekord_for_digest("sha256:deadbeef", b"rekor-test-key");
        assert_eq!(rec.kind, HASHEDREKORD_KIND);
        verify_hashedrekord(&rec, "deadbeef").unwrap();
        let mut bad = rec.clone();
        bad.spec.data.hash.value = "00".into();
        assert!(verify_hashedrekord(&bad, "deadbeef").is_err());
    }

    #[test]
    fn remote_rekor_refused() {
        let err = refuse_remote_rekor_url(Some("https://rekor.sigstore.dev"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("refusing remote Rekor"), "{err}");
        assert!(refuse_remote_rekor_url(None).is_ok());
        assert!(refuse_remote_rekor_url(Some("")).is_ok());
    }

    #[test]
    fn fulcio_certificate_rejected() {
        std::env::set_var("WVX_PROMOTION_HMAC_KEY", "rekor-test-key");
        let att = sample_att();
        let mut bundle = wrap_attestation(&att, b"rekor-test-key");
        bundle.verification_material.certificate = Some(SigstoreCertificate {
            raw_bytes: "Y2VydA==".into(),
        });
        let err = crate::verify_sigstore_bundle(&bundle, "demo.parse@1")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Fulcio"), "{err}");
    }

    #[test]
    fn local_tlog_roundtrip() {
        std::env::set_var("WVX_PROMOTION_HMAC_KEY", "rekor-test-key");
        let dir = std::env::temp_dir().join(format!(
            "wvx-rekor-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let att = sample_att();
        let rec = hashedrekord_for_digest(&att.payload_digest, b"rekor-test-key");
        let entry = append_transparency(
            &dir,
            TransparencyKind::Rekor,
            &att.implementation_id,
            &att.payload_digest,
            9,
        )
        .unwrap();
        let mut bundle = wrap_attestation(&att, b"rekor-test-key");
        attach_local_tlog(&mut bundle, &entry, &rec, b"rekor-test-key").unwrap();
        let log = crate::read_transparency_log(&dir).unwrap();
        verify_tlog_entries(&bundle, &log, &att.payload_digest).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
