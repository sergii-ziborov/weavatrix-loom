# Schemas

JSON Schema (draft 2020-12) for WVX wire documents.

| File | Purpose |
|------|---------|
| [`wvx.project.v0.1.json`](wvx.project.v0.1.json) | Project document (`.wvx.json`) |
| [`wvx.capability.v0.1.json`](wvx.capability.v0.1.json) | Registry capability contract |
| [`wvx.graph_patch.v0.1.json`](wvx.graph_patch.v0.1.json) | GraphPatch batch |

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
