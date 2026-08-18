//! SPDX 2.3 JSON projected from [`SoftwareBill`].
//!
//! This is a real SPDX-2.3 *document shape* (required fields + packages +
//! DESCRIBES). It is **not**:
//! - SPDX 3.0;
//! - a cargo-metadata / license scan;
//! - an NTIA “complete” minimum-element claim (`license*` stay `NOASSERTION`).

use crate::attestation::{sbom_from_implementation, SbomComponent, SoftwareBill};
use crate::evidence_artifact::sha256_hex;
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use wvx_ir::Implementation;

pub const SPDX_VERSION: &str = "SPDX-2.3";
pub const SPDX_DATA_LICENSE: &str = "CC0-1.0";
pub const SPDX_DOCUMENT_ID: &str = "SPDXRef-DOCUMENT";
pub const NOASSERTION: &str = "NOASSERTION";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpdxCreationInfo {
    pub created: String,
    pub creators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpdxChecksum {
    pub algorithm: String,
    #[serde(rename = "checksumValue")]
    pub checksum_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpdxExternalRef {
    #[serde(rename = "referenceCategory")]
    pub reference_category: String,
    #[serde(rename = "referenceType")]
    pub reference_type: String,
    #[serde(rename = "referenceLocator")]
    pub reference_locator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpdxPackage {
    #[serde(rename = "SPDXID")]
    pub spdx_id: String,
    pub name: String,
    #[serde(rename = "downloadLocation")]
    pub download_location: String,
    #[serde(rename = "filesAnalyzed")]
    pub files_analyzed: bool,
    #[serde(rename = "licenseConcluded")]
    pub license_concluded: String,
    #[serde(rename = "licenseDeclared")]
    pub license_declared: String,
    #[serde(rename = "copyrightText")]
    pub copyright_text: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "versionInfo"
    )]
    pub version_info: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checksums: Vec<SpdxChecksum>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "externalRefs"
    )]
    pub external_refs: Vec<SpdxExternalRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "primaryPackagePurpose"
    )]
    pub primary_package_purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpdxRelationship {
    #[serde(rename = "spdxElementId")]
    pub spdx_element_id: String,
    #[serde(rename = "relationshipType")]
    pub relationship_type: String,
    #[serde(rename = "relatedSpdxElement")]
    pub related_spdx_element: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpdxDocument {
    #[serde(rename = "spdxVersion")]
    pub spdx_version: String,
    #[serde(rename = "dataLicense")]
    pub data_license: String,
    #[serde(rename = "SPDXID")]
    pub spdx_id: String,
    pub name: String,
    #[serde(rename = "documentNamespace")]
    pub document_namespace: String,
    #[serde(rename = "creationInfo")]
    pub creation_info: SpdxCreationInfo,
    pub packages: Vec<SpdxPackage>,
    pub relationships: Vec<SpdxRelationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl SpdxDocument {
    pub fn digest(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).unwrap_or_default())
    }
}

pub fn spdx_from_implementation(imp: &Implementation, created_unix: u64) -> SpdxDocument {
    spdx_from_bill(&sbom_from_implementation(imp), created_unix)
}

pub fn spdx_from_bill(bill: &SoftwareBill, created_unix: u64) -> SpdxDocument {
    let root_id = spdx_ref("Package", &bill.implementation_id);
    let mut packages = vec![SpdxPackage {
        spdx_id: root_id.clone(),
        name: bill.implementation_id.clone(),
        download_location: NOASSERTION.into(),
        files_analyzed: false,
        license_concluded: NOASSERTION.into(),
        license_declared: NOASSERTION.into(),
        copyright_text: NOASSERTION.into(),
        version_info: None,
        checksums: Vec::new(),
        external_refs: Vec::new(),
        primary_package_purpose: Some("LIBRARY".into()),
        comment: Some("Loom implementation (root). License not scanned.".into()),
    }];
    let mut relationships = vec![SpdxRelationship {
        spdx_element_id: SPDX_DOCUMENT_ID.into(),
        relationship_type: "DESCRIBES".into(),
        related_spdx_element: root_id.clone(),
    }];

    let mut used = BTreeSet::from([root_id.clone()]);
    for (i, c) in bill.components.iter().enumerate() {
        let pkg = package_from_component(c, i);
        if !used.insert(pkg.spdx_id.clone()) {
            continue;
        }
        let rel = match c.kind.as_str() {
            "capability" => "OTHER",
            _ => "DEPENDS_ON",
        };
        relationships.push(SpdxRelationship {
            spdx_element_id: root_id.clone(),
            relationship_type: rel.into(),
            related_spdx_element: pkg.spdx_id.clone(),
        });
        packages.push(pkg);
    }

    let created = unix_to_spdx_created(created_unix);
    let mut doc = SpdxDocument {
        spdx_version: SPDX_VERSION.into(),
        data_license: SPDX_DATA_LICENSE.into(),
        spdx_id: SPDX_DOCUMENT_ID.into(),
        name: format!("loom-{}", bill.implementation_id),
        document_namespace: String::new(),
        creation_info: SpdxCreationInfo {
            created,
            creators: vec!["Tool: weavatrix-loom-wvx-registry-client".into()],
            comment: Some(
                "Projected from wvx.sbom.v0.1. Not SPDX 3.0. Not a license scan.".into(),
            ),
        },
        packages,
        relationships,
        comment: Some(
            "Same component set as `wvx registry sbom`. Lock enrichment is name-matched, not the full Cargo graph.".into(),
        ),
    };
    let ns_hash = doc
        .digest()
        .trim_start_matches("sha256:")
        .chars()
        .take(16)
        .collect::<String>();
    doc.document_namespace = format!(
        "https://weavatrix.com/spdx/2.3/{}/{}",
        sanitize_seg(&bill.implementation_id),
        ns_hash
    );
    doc
}

pub fn verify_spdx_document(doc: &SpdxDocument) -> Result<(), RegistryError> {
    if doc.spdx_version.starts_with("SPDX-3") {
        return Err(err("SPDX 3.x is not implemented"));
    }
    if doc.spdx_version != SPDX_VERSION {
        return Err(err(format!(
            "spdxVersion `{}` != `{SPDX_VERSION}`",
            doc.spdx_version
        )));
    }
    if doc.data_license != SPDX_DATA_LICENSE {
        return Err(err(format!(
            "dataLicense `{}` != `{SPDX_DATA_LICENSE}`",
            doc.data_license
        )));
    }
    if doc.spdx_id != SPDX_DOCUMENT_ID {
        return Err(err(format!(
            "SPDXID `{}` != `{SPDX_DOCUMENT_ID}`",
            doc.spdx_id
        )));
    }
    if doc.document_namespace.trim().is_empty() {
        return Err(err("documentNamespace is empty"));
    }
    if doc.creation_info.created.trim().is_empty() || doc.creation_info.creators.is_empty() {
        return Err(err("creationInfo is incomplete"));
    }
    if doc.packages.is_empty() {
        return Err(err("SPDX document has no packages"));
    }
    let mut ids = BTreeSet::new();
    for p in &doc.packages {
        if p.spdx_id.trim().is_empty() || p.name.trim().is_empty() {
            return Err(err("package missing SPDXID or name"));
        }
        if p.download_location.trim().is_empty() {
            return Err(err(format!(
                "package `{}` missing downloadLocation",
                p.spdx_id
            )));
        }
        if !ids.insert(p.spdx_id.clone()) {
            return Err(err(format!("duplicate SPDXID `{}`", p.spdx_id)));
        }
    }
    let describes = doc.relationships.iter().any(|r| {
        r.spdx_element_id == SPDX_DOCUMENT_ID
            && r.relationship_type == "DESCRIBES"
            && ids.contains(&r.related_spdx_element)
    });
    if !describes {
        return Err(err("missing DOCUMENT DESCRIBES <package> relationship"));
    }
    Ok(())
}

fn package_from_component(c: &SbomComponent, index: usize) -> SpdxPackage {
    let mut id = spdx_ref(&c.kind, &c.name);
    if id == "SPDXRef--" {
        id = format!("SPDXRef-Component-{index}");
    }
    let version = if c.version.trim().is_empty() {
        None
    } else {
        Some(c.version.clone())
    };
    let mut external_refs = Vec::new();
    if let Some(purl) = &c.purl {
        external_refs.push(SpdxExternalRef {
            reference_category: "PACKAGE-MANAGER".into(),
            reference_type: "purl".into(),
            reference_locator: purl.clone(),
        });
    }
    let mut checksums = Vec::new();
    if let Some(d) = &c.digest {
        let hex = d.trim_start_matches("sha256:");
        if !hex.is_empty() {
            checksums.push(SpdxChecksum {
                algorithm: "SHA256".into(),
                checksum_value: hex.into(),
            });
        }
    }
    let download = match (c.kind.as_str(), c.purl.as_deref(), version.as_deref()) {
        ("upstream", Some(p), _) if p.starts_with("pkg:cargo/") => {
            if let Some((name, ver)) = p
                .strip_prefix("pkg:cargo/")
                .and_then(|rest| rest.split_once('@'))
            {
                format!("https://crates.io/crates/{name}/{ver}")
            } else {
                NOASSERTION.into()
            }
        }
        _ => NOASSERTION.into(),
    };
    let purpose = match c.kind.as_str() {
        "capability" => "OTHER",
        _ => "LIBRARY",
    };
    SpdxPackage {
        spdx_id: id,
        name: c.name.clone(),
        download_location: download,
        files_analyzed: false,
        license_concluded: NOASSERTION.into(),
        license_declared: NOASSERTION.into(),
        copyright_text: NOASSERTION.into(),
        version_info: version,
        checksums,
        external_refs,
        primary_package_purpose: Some(purpose.into()),
        comment: Some(format!("wvx.sbom.kind={}", c.kind)),
    }
}

fn spdx_ref(kind: &str, name: &str) -> String {
    format!("SPDXRef-{}-{}", sanitize_seg(kind), sanitize_seg(name))
}

fn sanitize_seg(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// SPDX `created` is UTC `YYYY-MM-DDThh:mm:ssZ`.
pub fn unix_to_spdx_created(unix: u64) -> String {
    let secs = unix % 86_400;
    let days = (unix / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    let hh = secs / 3600;
    let mm = (secs % 3600) / 60;
    let ss = secs % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Howard Hinnant civil-from-days (proleptic Gregorian, Unix epoch day 0 = 1970-01-01).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn err(msg: impl Into<String>) -> RegistryError {
    RegistryError::Parse(Path::new("<spdx>").to_path_buf(), msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{AdapterRef, CapabilityRef, ImplementationSource};

    fn sample_imp() -> Implementation {
        Implementation {
            id: "demo.parse".into(),
            version: "1".into(),
            capability: CapabilityRef {
                id: "data.json.parse".into(),
                version: "1".into(),
            },
            source: ImplementationSource {
                kind: "crates.io".into(),
                package: "serde_json".into(),
                package_version: "1.0.0".into(),
                notes: None,
            },
            adapter: Some(AdapterRef {
                crate_name: "wvx-adapters".into(),
                execution: "native-rust".into(),
            }),
            status: wvx_ir::LifecycleStatus::Candidate,
            evidence: Default::default(),
            evidence_artifact: None,
            notes: None,
            sdk: None,
            source_ref: None,
            conformance_profile: Some("json-rfc8259-core-v1".into()),
        }
    }

    #[test]
    fn created_epoch() {
        assert_eq!(unix_to_spdx_created(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_spdx_created(1_609_459_200), "2021-01-01T00:00:00Z");
    }

    #[test]
    fn spdx_23_from_impl() {
        let doc = spdx_from_implementation(&sample_imp(), 1_609_459_200);
        verify_spdx_document(&doc).unwrap();
        assert_eq!(doc.spdx_version, SPDX_VERSION);
        assert!(doc.packages.iter().any(|p| p.name == "demo.parse@1"));
        assert!(doc.packages.iter().any(|p| p.name == "serde_json"));
        let up = doc
            .packages
            .iter()
            .find(|p| p.name == "serde_json")
            .unwrap();
        assert!(up
            .external_refs
            .iter()
            .any(|r| r.reference_locator.starts_with("pkg:cargo/serde_json@")));
        assert!(up.download_location.contains("crates.io"));
        assert_eq!(up.license_concluded, NOASSERTION);
        assert!(doc
            .relationships
            .iter()
            .any(|r| r.relationship_type == "DESCRIBES"));
        let mut v3 = doc.clone();
        v3.spdx_version = "SPDX-3.0".into();
        assert!(verify_spdx_document(&v3)
            .unwrap_err()
            .to_string()
            .contains("3.x"));
    }
}
