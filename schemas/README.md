# Schemas

JSON Schema (draft 2020-12) for WVX wire documents.

| File | Purpose |
|------|---------|
| [`wvx.project.v0.1.json`](wvx.project.v0.1.json) | Project document (`.wvx.json`) |
| [`wvx.capability.v0.1.json`](wvx.capability.v0.1.json) | Registry capability contract |
| [`wvx.implementation.v0.1.json`](wvx.implementation.v0.1.json) | Registry implementation manifest (+ lifecycle / evidence / `source_ref`) |
| [`wvx.facts.v0.1.json`](wvx.facts.v0.1.json) | Weavatrix → Forge facts interchange (legacy) |
| [`wvx.facts.v0.2.json`](wvx.facts.v0.2.json) | Facts v0.2 (digest, qualified name, spans, docs, cfg, unsafe, effects) |
| [`wvx.graph_patch.v0.1.json`](wvx.graph_patch.v0.1.json) | GraphPatch batch |
| [`wvx.evidence_artifact.v0.1.json`](wvx.evidence_artifact.v0.1.json) | Evidence artifact (legacy) |
| [`wvx.evidence_artifact.v0.2.json`](wvx.evidence_artifact.v0.2.json) | Evidence artifact v0.2 (digests + environment + case_results) |

## Contract tests

Hand-authored schemas are guarded by **`wvx-schema-contract`**:

```bash
cargo test -p wvx-schema-contract
```

Checks:

1. Schema `required` fields present on pilot fixtures + registry samples  
2. Serde roundtrip for `Project`, `Implementation`, `GraphPatch`, `EvidenceArtifact`  
3. Constants (`wvx.project.v0.1`, `wvx.evidence.v0.2`) stay aligned with schema `const`

Full schemars generation can replace hand schemas later; until then these tests
fail closed when wire contracts drift.

## Source of truth

1. **Runtime/IR:** Rust types in `crates/wvx-ir` and `crates/wvx-types` (serde).
2. **Wire contracts for tools:** these JSON Schemas.
3. **Examples:** `fixtures/pilot-json-pipeline.wvx.json`, `registry-dev/capabilities/*.json`.

If schema and Rust disagree, **fix Rust**, then update the schema.

## Conventions (v0.1)

- `schema_version` for projects is exactly `wvx.project.v0.1`.
- Port field is JSON key `"type"` (Rust field `ty`).
- Type unit tags: snake_case on the wire (`json_value`); dotted aliases (`json.value`) are accepted by the deserializer.
- `instance.ui` is layout-only (ADR-0003).
- Capabilities may be embedded on the project or hydrated from a local registry at run/validate time.

## Validate fixture (optional)

With Node / `ajv-cli` (or any draft-2020-12 validator):

```bash
npx --yes ajv-cli validate -s schemas/wvx.project.v0.1.json -d fixtures/pilot-json-pipeline.wvx.json --spec=draft2020
```

Rust hosts use serde + `wvx-validator` for semantic checks beyond JSON shape.
