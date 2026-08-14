# Milestone 1 — Truthful Registry

**Date:** 2026-08-14  
**Status:** In force for `registry-dev`

## Rules

1. **No false capability claims**
   - ASCII transforms must not sit on Unicode capabilities.
   - Reference JSON parser claims **subset** profile only (`json-subset-pilot-v1`), not full RFC 8259.
   - `path_set` uses one shared semantics: **single-segment JSON Pointer leaf assign** (`json-path-set-single-segment-v1`).
   - Hash / compression multi-impls stay **`candidate`** until a **shared suite artifact** exists.

2. **Versioned conformance profiles** live under `registry-dev/profiles/`.  
   Each profile lists vectors, negative vectors, error families, guarantees, limitations, suite digest.

3. **Evidence artifacts** under `registry-dev/evidence/artifacts/{impl}.json` are required for  
   `status ∈ {conformant, admitted}`.  
   Audit **computes** justified status from the artifact (suite pass + axes), not free-form manifest strings.

4. **CI invariant**

```text
for each implementation with status >= conformant:
  evidence artifact exists
  subject digest matches
  capability + profile match
  all required suites pass
```

```bash
cargo run -p wvx-cli -- registry truthful   # must PASS
cargo run -p wvx-cli -- registry check
```

## Profiles (pilot set)

| Profile | Capability |
|---------|------------|
| `json-rfc8259-core-v1` | `data.json.parse@1` |
| `json-subset-pilot-v1` | `data.json.parse@1` (narrow) |
| `json-path-set-single-segment-v1` | `data.json.path_set@1` |
| `unicode-uppercase-15.1-v1` | `data.text.unicode_uppercase@1` |
| `ascii-uppercase-v1` | `data.text.ascii_uppercase@1` |
| `sha256-fips180-4-v1` | `data.hash.sha256@1` (suite required before conformant) |

## Example conformant

`serde-json.parse-owned@1` ships with a sample evidence artifact (Gate A pilot suite).  
All other pilot impls are intentionally **`candidate`** until suites are recorded.
