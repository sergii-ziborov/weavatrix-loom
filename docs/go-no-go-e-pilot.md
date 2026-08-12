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
cargo run -p wvx-cli -- bench --iterations 200 --warmup 20 -o .lab/bench.json
```

- Runs pilot parse (3), serialize (2), path_set (2) handlers
- **pass** = all cases execute without error (not a flaky timing CI gate)
- Emits host provenance (os/arch/version) + input fingerprint

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
4. No SBOM / sigstore / cargo-vet integration yet.

## How to re-run

```bash
cargo test -p wvx-conformance bench
cargo run -p wvx-cli -- bench -o .lab/bench.json
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
