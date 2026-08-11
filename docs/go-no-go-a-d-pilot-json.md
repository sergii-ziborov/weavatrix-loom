# Go / No-Go notes — Gates A & D (JSON pilot)

**Date:** 2026-08-12 (updated: third parse backend)  
**Scope:** Pilot A only (`Input → JSON Parse → Path Set → Serialize → Output`)  
**Harness:** `wvx-conformance` + `wvx-cli conformance [--golden]`  
**Verdict summary**

| Gate | Title | Pilot verdict | Confidence |
|------|--------|---------------|------------|
| **A** | Interchangeability | **Go (parse)** | High for **3** independent parse backends + 3 serialize; path_set still single-impl |
| **D** | Runtime equivalence | **Go (pilot)** | High for compact combos including `json-crate.parse@1` |

These notes do **not** close Gates B, C, or E. They only record evidence for the v0.1 transform pilot.

---

## Gate A — Interchangeability

### Criterion (master plan)

> Multiple implementations of one capability pass a common contract and can be swapped without changing the capability graph or bindings.

### What we measured

| Capability | Implementations under test | Contract check |
|------------|----------------------------|----------------|
| `data.json.parse@1` | `serde-json.parse-owned@1`, `wvx.reference.json-parse@1`, **`json-crate.parse@1`** (crates.io `json` 0.12) | Shared parse vectors (7 cases × **3** impls) |
| `data.json.serialize@1` | `serde-json.serialize@1`, `wvx.reference.json-serialize@1`, `wvx.reference.json-serialize-pretty@1` | Round-trip semantic equality |
| `data.json.path_set@1` | `wvx.reference.path-set@1` | Single reference (no swap pair yet) |
| `io.input.bytes@1` / `io.output.bytes@1` | reference only | Seed / sink |

**Conformance run (2026-08-12, post third parse):**

```text
cargo run -p wvx-cli -- conformance
→ conformance: 25 cases, 0 failed · PASS
```

Breakdown:

- 21 parse cases (**3** impls × 7 vectors: object, nested, array, number, string, bool, null)
- 3 serialize round-trips (2 compact + 1 pretty; pretty checked for semantic re-parse)
- 1 path_set case (`/tag` → `"loom"`)

**Graph swap (no binding change):**

```bash
# default serde parse
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json

# alternate parse backends, same WVX graph
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=wvx.reference.json-parse@1

cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=json-crate.parse@1
```

Studio: Inspector → pick another implementation → **Run** (capability + edges unchanged).

### Gate A decision

| Question | Answer |
|----------|--------|
| Can ≥2 impls of one capability share vectors? | **Yes** — parse (**3**), serialize (3) |
| Does swap leave graph/bindings intact? | **Yes** — only `instance.implementation` changes |
| Three independent parse codebases? | **Yes** — `serde_json`, in-tree reference lite, crates.io `json` |
| path_set multi-impl? | **No** — single reference (does not block parse Gate A) |

**Verdict: Go (parse)** — three independent parsers fulfill `data.json.parse@1` on the shared suite without rewiring the graph.  
Serialize already has three backends (two compact + pretty). Full product-wide Gate A for every pilot capability still needs multi-impl `path_set` / I/O if those become swap-critical.

### Residual risks for A

1. Pretty vs compact serialize intentionally differ as **bytes** but agree as **JSON values** — UI/trace must show which impl ran.
2. Boundary normalization maps `json` crate values into `serde_json::Value`; exotic number edge cases should keep expanding the vector set.
3. No automated property tests against a large JSON corpus yet.

---

## Gate D — Runtime equivalence

### Criterion (master plan)

> Dynamic playground and static exported Rust have the same semantics on the pilot pipeline.

### What we measured

Golden harness (`wvx-conformance`):

1. Run playground (`HandlerRegistry::with_pilot`) on pilot fixture + input `{"hello":"world"}`.
2. Export same project (`export_to_directory` + vendored `wvx-adapters`).
3. `cargo check` + `cargo run` with the same input.
4. Compare **JSON values** of pipeline output (not raw byte identity).

**Combos covered by tests:**

| Parse impl | Serialize impl | Role |
|------------|----------------|------|
| default (`serde-json.parse-owned@1`) | default (`serde-json.serialize@1`) | primary path |
| `wvx.reference.json-parse@1` | default | parse swap |
| `json-crate.parse@1` | default | third parse / Gate A |
| default | `wvx.reference.json-serialize@1` | serialize swap |
| both reference compact | | dual swap |

```bash
cargo test -p wvx-conformance golden
# or
cargo run -p wvx-cli -- conformance --golden   # slower: cargo per combo
```

**Expected pilot output (semantic):**

```json
{ "hello": "world", "tag": "loom" }
```

(`path_set` config on the fixture: `path=/tag`, `value=loom`.)

### Gate D decision

| Question | Answer |
|----------|--------|
| Default pipeline playground ≡ static export? | **Yes** (golden tests green as of 2026-08-12) |
| Swap combos (compact) also equivalent? | **Yes** for the four compact combos in `run_all_goldens` |
| Byte-identical stdout? | **Not required** — comparison is JSON value equality (key order / whitespace may differ) |
| Pretty serialize included in golden matrix? | **No** — pretty is conformance-only; static export default remains compact adapters |

**Verdict: Go (pilot)** — for the JSON pilot and the adapter set shipped in `wvx-adapters` / compiler vendor, playground and export agree.

### Residual risks for D

1. Golden invokes a full `cargo` toolchain; CI must have Rust installed and network for first dep fetch (vendored adapters reduce this).
2. New adapters must be registered in **both** runtime pilot handlers and compiler emit map, or Gate D will fail loudly — keep dual registration as a checklist item.
3. Equivalence is **JSON semantic**, not bit-identical serialization.

---

## How to re-run evidence

```bash
cd weavatrix-loom

# Gate A — capability vectors
cargo run -p wvx-cli -- conformance
cargo test -p wvx-conformance pilot_capability_conformance

# Gate D — playground vs export
cargo test -p wvx-conformance golden
cargo run -p wvx-cli -- conformance --golden
```

Fixture: `fixtures/pilot-json-pipeline.wvx.json`  
Implementations catalog: `cargo run -p wvx-cli -- implementations`  
Registry: `registry-dev/`

---

## Related gates (not claimed)

| Gate | Status | Why not claimed |
|------|--------|-----------------|
| **B** Visual advantage | Open | No timed UX study vs raw crates |
| **C** Forge economics | Open | Inventory/extract only; no auto adapter cost study |
| **E** Registry trust | Open | No reproducible benchmark/provenance admission pipeline |

---

## Recommendation

1. Treat **A (parse Go)** + **D (pilot Go)** as closed for the JSON pilot story; product is still broader than this gate pair.
2. Next evidence upgrades:
   - Expand JSON vectors (unicode, deep trees, invalid-input error codes).
   - Optional second `path_set` backend if that capability becomes swap-critical.
   - CI: `.github/workflows/ci.yml` already runs `wvx conformance` + `cargo test -p wvx-conformance` on every PR.
3. Do not expand registry beyond pure transforms until Gate D remains green when new adapters land.
