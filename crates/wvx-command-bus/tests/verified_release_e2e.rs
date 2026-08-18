//! Mandatory CI gate: real source → real profile → exact cases →
//! source-bound artifact → VerifiedImplementation → compile_release.
//!
//! Uses a temporary Registry. Never writes to `registry-dev`.

use std::path::PathBuf;
use wvx_command_bus::{load_project_path, project_export_rust_release, LivePromotionCollector};
use wvx_registry_client::{
    materialize_temp_registry, promote_implementation_with_collector, verify_implementation,
    LocalRegistry, PromoteRequest,
};

fn monorepo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn verified_end_to_end_release_fixture() {
    let src = monorepo_root().join("registry-dev");
    let dest = std::env::temp_dir().join(format!(
        "wvx-e2e-release-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    materialize_temp_registry(
        &src,
        &dest,
        &["data.json.parse@1", "io.input.bytes@1"],
        &["json-rfc8259-core-v1"],
        &["serde-json.parse-owned@1", "wvx.reference.io-input-bytes@1"],
    )
    .expect("temp registry");

    let reg = LocalRegistry::open(&dest).unwrap();
    let imp = reg
        .find_implementation("serde-json.parse-owned@1")
        .unwrap()
        .expect("impl");

    let req = PromoteRequest {
        implementation_id: "serde-json.parse-owned@1".into(),
        profile_id: Some("json-rfc8259-core-v1".into()),
        target_status: "conformant".into(),
        human: None,
        apply: true,
        notes: vec!["verified-release-e2e".into()],
        signed_reports: None,
    };
    let collector = LivePromotionCollector;
    let result =
        promote_implementation_with_collector(&dest, imp, &req, Some(&collector)).expect("promote");
    assert!(
        result.ok,
        "promote failed: steps={:?} findings={:?}",
        result.steps, result.findings
    );
    assert_eq!(result.new_status, "conformant");
    assert!(result.verified.is_some());
    assert!(result.artifact_path.is_some());

    let reg = LocalRegistry::open(&dest).unwrap();
    let imp = reg
        .find_implementation("serde-json.parse-owned@1")
        .unwrap()
        .unwrap();
    let verified = verify_implementation(&dest, &imp).expect("re-verify");
    assert!(verified.check.ok);
    assert_eq!(
        verified.artifact.schema_version,
        wvx_registry_client::EVIDENCE_SCHEMA
    );
    assert!(!verified
        .artifact
        .digests
        .implementation_source_tree
        .is_empty());
    assert!(!verified.artifact.digests.adapter_source.is_empty());
    assert!(!verified.artifact.digests.cargo_lock.is_empty());
    assert!(!verified.artifact.digests.package_checksum.is_empty());
    assert!(!verified.artifact.digests.profile_case_ids.is_empty());

    let expected_ids = wvx_registry_client::profile_case_ids(
        &wvx_registry_client::load_profile(&dest, "json-rfc8259-core-v1")
            .unwrap()
            .0,
    );
    let recorded: Vec<_> = verified
        .artifact
        .case_results
        .iter()
        .map(|c| c.case_id.clone())
        .collect();
    let mut recorded_sorted = recorded.clone();
    recorded_sorted.sort();
    recorded_sorted.dedup();
    assert_eq!(
        recorded_sorted, expected_ids,
        "artifact must record exact profile case IDs"
    );

    let project =
        load_project_path(&monorepo_root().join("fixtures/verified-json-parse-release.wvx.json"))
            .expect("fixture");
    let resp =
        project_export_rust_release(&project, &[verified], Some(&reg)).expect("compile_release");
    assert!(resp.ok, "compile_release: {:?}", resp.diagnostics);
    let report = resp.data.expect("compile report");
    assert!(
        report
            .workspace
            .files
            .iter()
            .any(|f| f.relative_path.ends_with("lib.rs")),
        "expected generated rust"
    );

    let _ = std::fs::remove_dir_all(&dest);
}
