# Go / No-Go notes — Gate E (Registry trust, JSON pilot)

**Date:** 2026-08-12  
**Scope:** Pilot transform implementations in `registry-dev`  
**Harness:** `wvx bench`, `wvx registry check`, `wvx registry admit`  
**Verdict summary**

| Gate | Title | Pilot verdict | Confidence |
|------|--------|---------------|------------|
| **E** | Registry trust | **Go (pilot lab)** | Medium — automated bench + provenance + human admit path exist; **not** a public production trust store |

These notes do **not** replace Gates A–D. They record a **minimal** Gate E path.

---

## Criterion (master plan)

> Registry entries that claim trust (especially `admitted`) carry multi-fact evidence and human policy where required; overclaim is rejected.

## What we built

| Piece | Command / path | Role |
|-------|----------------|------|
| Lifecycle + axes | manifests `status` / `evidence` | ADR-0008 chips |
| Overclaim audit | `wvx registry check` | declared ≤ justified |
| Microbench | `wvx bench [-o file]` | `benchmark` axis input (execution success) |
| Provenance | `registry-dev/evidence/*.provenance.json` | host + review + bench fingerprint |
| Human admit | `wvx registry admit … [--apply]` | fail-closed promotion to `admitted` |

### Bench (automated)

```bash
cargo run -p wvx-cli --release -- bench --iterations 200 --warmup 20 -o .lab/bench.json
# Cases include tiny + 64KiB string + twitter_like/catalog_like (synthetic shapes).
# Optional: $env:RUSTFLAGS="-C target-cpu=native" for sonic/simd-json.
```

- **49 cases** (2026-08-19): tiny parse, 64 KiB string, **twitter_like** (79 KiB) +
  **catalog_like** (24 KiB) synthetic shapes, serialize, path_set, hash, gzip, codecs.
  Not copyrighted serde-json-benchmark dumps.
- **pass** = all cases execute without error (not a flaky timing CI gate)
- Emits host provenance (os/arch/version, RUSTFLAGS / `target-cpu=native`) + input fingerprint
- Rankings on this lab (release, Windows x86_64; ns are host-dependent):
  - parse twitter_like: **sonic-rs** faster than serde (~1.8×); simd-json only wins
    with `-C target-cpu=native` and still trails sonic
  - 64 KiB *one string*: simd-json loses (adapter copies into `serde_json::Value`)
  - hash 64 KiB: **blake3** serial faster than sha256; rayon-parallel slower at this size
  - gzip 64 KiB / gunzip: **zlib-rs** faster than flate2/miniz

### Human admit (fail-closed)

```bash
cargo run -p wvx-cli -- registry admit serde-json.parse-owned@1 \
  --reviewer "Sergii Ziborov" \
  --human-ack "Reviewed pilot Gate E evidence (conformance + bench)" \
  --security-ack "Pilot-only security posture accepted for lab admit" \
  --reason "Demonstrate Gate E human path" \
  --bench-file .lab/bench.json
  # add --apply to rewrite registry-dev/implementations/*.json
```

Requires:

1. Non-empty reviewer / human_ack / security_ack / reason  
2. Successful bench file (`data.ok == true`)  
3. Existing `conformance: pass` + adapter  
4. Policy then sets `benchmark`+`security` pass and `status: admitted` only if **justified**

Dry-run (default) writes provenance under `registry-dev/evidence/` and mirrors `.lab/admissions/` (gitignored).  
`--apply` updates the public-ish registry-dev manifest (use deliberately).

## Gate E decision

| Question | Answer |
|----------|--------|
| Can we measure pilot impls reproducibly enough for evidence? | **Yes** — microbench + fingerprint (host-dependent timings) |
| Can overclaim of `admitted` be blocked? | **Yes** — check + admit policy |
| Is human review required for admit? | **Yes** — explicit acks |
| Public production registry trust? | **No** — pilot lab only |

**Verdict: Go (pilot lab)** — the trust *pipeline* is real and fail-closed for overclaim; absolute performance and full provenance fleet remain residual.

### Residual risks

1. Absolute `mean_ns` is not comparable across machines; only pass/fail of execution is policy input.  
2. Security ack is process, not an automated scanner.  
3. Default registry-dev stays mostly `conformant` / `candidate` unless someone runs `--apply`.  
4. HMAC attest / local hashedrekord / SPDX 2.3 exist; they are **not** Fulcio, public Rekor, or a license scan.

## How to re-run

```bash
cargo test -p wvx-conformance --lib bench
cargo run -p wvx-cli --release -- bench --iterations 200 --warmup 20 -o .lab/bench.json
cargo run -p wvx-cli -- registry check
# optional lab admit (dry-run):
cargo run -p wvx-cli -- registry admit serde-json.parse-owned@1 \
  --reviewer "Sergii Ziborov" \
  --human-ack "Reviewed pilot Gate E evidence (conformance + bench)" \
  --security-ack "Pilot-only security posture accepted for lab admit" \
  --reason "Gate E dry-run" \
  --bench-file .lab/bench.json
```

## Related

- Gate A/D: [`go-no-go-a-d-pilot-json.md`](go-no-go-a-d-pilot-json.md)  
- Admission policy (overclaim): [`admission-pilot.md`](admission-pilot.md)  
- ADR-0007 / ADR-0008
