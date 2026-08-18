//! Offline Sigstore-shaped bundle: in-toto Statement + DSSE envelope.
//!
//! This is **not** Fulcio + Rekor. Signatures use `WVX_PROMOTION_HMAC_KEY`
//! (same as promotion reports). The JSON shape matches a Sigstore bundle
//! enough that a later real key material can replace `verificationMaterial`.

use crate::attestation::SignedAttestation;
use crate::signed::{hmac_sha256, promotion_hmac_key};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const IN_TOTO_STATEMENT: &str = "https://in-toto.io/Statement/v1";
pub const PREDICATE_TYPE: &str = "https://weavatrix.com/attestation/v0.1";
pub const DSSE_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";
pub const BUNDLE_MEDIA: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InTotoSubject {
    pub name: String,
    pub digest: BTreeMapSha,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BTreeMapSha {
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InTotoStatement {
    #[serde(rename = "_type")]
    pub type_: String,
    pub subject: Vec<InTotoSubject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: SignedAttestation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DsseSignature {
    pub keyid: String,
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DsseEnvelope {
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    pub payload: String,
    pub signatures: Vec<DsseSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SigstoreVerificationMaterial {
    pub offline: bool,
    pub scheme: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SigstoreBundle {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "dsseEnvelope")]
    pub dsse_envelope: DsseEnvelope,
    #[serde(rename = "verificationMaterial")]
    pub verification_material: SigstoreVerificationMaterial,
}

/// DSSE Pre-Authentication Encoding (PAE).
pub fn dsse_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"DSSEv1 ");
    out.extend_from_slice(payload_type.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload_type.as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload);
    out
}

pub fn wrap_attestation(att: &SignedAttestation, key: &[u8]) -> SigstoreBundle {
    let digest = att
        .artifact_digest
        .trim_start_matches("sha256:")
        .to_string();
    let statement = InTotoStatement {
        type_: IN_TOTO_STATEMENT.into(),
        subject: vec![InTotoSubject {
            name: att.implementation_id.clone(),
            digest: BTreeMapSha { sha256: digest },
        }],
        predicate_type: PREDICATE_TYPE.into(),
        predicate: att.clone(),
    };
    let payload_bytes = serde_json::to_vec(&statement).unwrap_or_default();
    let pae = dsse_pae(DSSE_PAYLOAD_TYPE, &payload_bytes);
    let sig = hmac_sha256(key, &pae);
    SigstoreBundle {
        media_type: BUNDLE_MEDIA.into(),
        dsse_envelope: DsseEnvelope {
            payload_type: DSSE_PAYLOAD_TYPE.into(),
            payload: b64_encode(&payload_bytes),
            signatures: vec![DsseSignature {
                keyid: "wvx-hmac".into(),
                sig: b64_encode(sig.as_bytes()),
            }],
        },
        verification_material: SigstoreVerificationMaterial {
            offline: true,
            scheme: "hmac-sha256".into(),
            note: "Offline predates Fulcio/Rekor. Verify with WVX_PROMOTION_HMAC_KEY.".into(),
        },
    }
}

pub fn verify_sigstore_bundle(
    bundle: &SigstoreBundle,
    expected_impl: &str,
) -> Result<InTotoStatement, RegistryError> {
    if bundle.media_type != BUNDLE_MEDIA {
        return Err(err(format!(
            "sigstore mediaType `{}` != `{BUNDLE_MEDIA}`",
            bundle.media_type
        )));
    }
    let env = &bundle.dsse_envelope;
    if env.payload_type != DSSE_PAYLOAD_TYPE {
        return Err(err("sigstore DSSE payloadType mismatch"));
    }
    let payload = b64_decode(&env.payload).ok_or_else(|| err("sigstore payload is not base64"))?;
    let Some(key) = promotion_hmac_key() else {
        return Err(err(
            "sigstore rejected: WVX_PROMOTION_HMAC_KEY is not set (fail-closed)",
        ));
    };
    let pae = dsse_pae(&env.payload_type, &payload);
    let expect = hmac_sha256(&key, &pae);
    let sig = env
        .signatures
        .first()
        .ok_or_else(|| err("sigstore envelope has no signatures"))?;
    let got = b64_decode(&sig.sig).ok_or_else(|| err("sigstore signature is not base64"))?;
    if got != expect.as_bytes() {
        return Err(err("sigstore HMAC mismatch"));
    }
    let stmt: InTotoStatement =
        serde_json::from_slice(&payload).map_err(|e| err(format!("sigstore in-toto JSON: {e}")))?;
    if stmt.type_ != IN_TOTO_STATEMENT {
        return Err(err("in-toto _type mismatch"));
    }
    if stmt.predicate_type != PREDICATE_TYPE {
        return Err(err("in-toto predicateType mismatch"));
    }
    if stmt.predicate.implementation_id != expected_impl {
        return Err(err(format!(
            "in-toto subject impl `{}` != `{expected_impl}`",
            stmt.predicate.implementation_id
        )));
    }
    Ok(stmt)
}

fn err(msg: impl Into<String>) -> RegistryError {
    RegistryError::Parse(Path::new("<sigstore>").to_path_buf(), msg.into())
}

fn b64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(T[(a >> 2) as usize] as char);
        out.push(T[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[(((b & 15) << 2) | (c >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(c & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::new();
    for chunk in cleaned.as_bytes().chunks(4) {
        let (b0, b1, b2, b3) = (
            val(chunk[0])?,
            val(chunk[1])?,
            val(chunk.get(2).copied().unwrap_or(b'='))?,
            val(chunk.get(3).copied().unwrap_or(b'='))?,
        );
        out.push((b0 << 2) | (b1 >> 4));
        if chunk.get(2).copied() != Some(b'=') {
            out.push((b1 << 4) | (b2 >> 2));
        }
        if chunk.get(3).copied() != Some(b'=') {
            out.push((b2 << 6) | b3);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestation::sign_attestation;
    use crate::evidence_artifact::EvidenceArtifact;

    #[test]
    fn dsse_roundtrip() {
        std::env::set_var("WVX_PROMOTION_HMAC_KEY", "sigstore-test-key");
        let art = EvidenceArtifact {
            schema_version: crate::EVIDENCE_SCHEMA.into(),
            implementation_id: "demo.parse@1".into(),
            capability_key: "data.json.parse@1".into(),
            conformance_profile: "json-rfc8259-core-v1".into(),
            subject_digest: "sha256:abc".into(),
            profile_suite_digest: String::new(),
            suite_results: vec![],
            axes: Default::default(),
            recorded_at_unix: 1,
            notes: vec![],
            digests: crate::evidence_artifact::EvidenceDigests {
                subject: "sha256:abc".into(),
                implementation_source_tree: "sha256:src".into(),
                ..Default::default()
            },
            environment: Default::default(),
            case_results: vec![],
        };
        let att = sign_attestation(
            "demo.parse@1",
            "json-rfc8259-core-v1",
            &art,
            "sha256:sbom",
            1,
            b"sigstore-test-key",
        );
        let bundle = wrap_attestation(&att, b"sigstore-test-key");
        let stmt = verify_sigstore_bundle(&bundle, "demo.parse@1").unwrap();
        assert_eq!(stmt.predicate.implementation_id, "demo.parse@1");
        let mut bad = bundle.clone();
        bad.dsse_envelope.signatures[0].sig = b64_encode(b"nope");
        assert!(verify_sigstore_bundle(&bad, "demo.parse@1").is_err());
    }
}
