# wvx-schema-contract

Internal contract tests: JSON Schema **required** fields vs fixtures, and
Rust IR / GraphPatch / EvidenceArtifact **serde roundtrips**.

Schemas under monorepo `schemas/` remain hand-authored for v0.1/v0.2; these
tests fail if code and schema diverge on required wire fields.

```bash
cargo test -p wvx-schema-contract
```
