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

   **v0.2** (`wvx.evidence.v0.2`) is current. Mint via:

   ```bash
   cargo run -p wvx-cli -- registry mint-evidence serde-json.parse-owned@1 --profile json-rfc8259-core-v1
   cargo run -p wvx-cli -- registry verify-evidence serde-json.parse-owned@1
   ```

   v0.2 requires: digests (source tree, adapter source closure, Cargo.lock, package
   checksum, source_ref revision, exact profile case IDs, upstream package, capability
   contract, profile, suite, subject), environment (target, toolchain, features, runner
   identity), case-by-case results, timestamp. Verifier **loads the profile** and
   **recomputes** those digests. v0.1 remains readable for migration.

   Public `promote` does not accept invented `ok=true` cases or evidence booleans.
   It runs live collectors (or HMAC-signed reports) and writes via staging + lock
   + atomic rename. Dry-run is read-only.

5. **Unified promotion** (`registry promote`) is the single transaction:

   ```text
   build → profile suite → bench → license/security → mint artifact
   → verify → optional human (admitted) → manifest → truthful audit
   ```

   Release compile accepts only `VerifiedImplementation` (`compile_release`).  
   Resolver release policy requires **Pass** axes + verified artifact path.

   `wvx registry sigstore` wraps a signed attestation as an in-toto Statement v1
   inside a DSSE envelope (`application/vnd.dev.sigstore.bundle.v0.3+json`).
   `wvx registry rekor` adds a Rekor **hashedrekord** v0.0.1 and local
   `tlogEntries`. The signature is still `WVX_PROMOTION_HMAC_KEY`.
   A Fulcio certificate in the bundle is **rejected**. `WVX_REKOR_URL` is
   **refused** — HMAC is not a Fulcio identity and is not uploaded to public Rekor.

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

`serde-json.parse-owned@1` is the first **live-promotable** parse implementation
(`json-rfc8259-core-v1`). A checked-in sample artifact is not a substitute for
`wvx registry promote` / the `verified_release_e2e` CI gate. All other pilot impls
remain **`candidate`** until a live suite artifact exists.
