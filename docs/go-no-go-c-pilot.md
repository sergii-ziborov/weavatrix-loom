# Go / No-Go notes — Gate C (Forge economics) pilot

**Date:** 2026-08-12  
**Scope:** FORGE-004 AST extract · FORGE-007 ontology match · FORGE-008 compileable adapters  
**Status:** **Go (pilot fixture harness)** — not production marketplace Go

## Criterion (roadmap §11.9, pilot-scaled)

| Metric | Pilot threshold | How measured |
|--------|-----------------|--------------|
| API extraction recall | ≥ 0.8 | Expected public fns found via AST extract |
| Capability mapping accuracy | ≥ 0.8 | Ontology match reuses expected capability |
| Generated adapter compile rate | ≥ 0.5 | `cargo check` on FORGE-008 packages |
| False semantic mappings | 0 | `exact_shape` to wrong capability |
| Evidence integrity | hard | No AI-only pass / no auto-admit |

Full Gate C (5 external conformant impls, human minutes, overhead) remains open.

## Evidence

```bash
# AST extract
cargo run -p wvx-cli -- forge extract crates/wvx-adapters

# Ontology match
cargo run -p wvx-cli -- forge match crates/wvx-adapter-external-demo

# Compileable adapters
cargo run -p wvx-cli -- forge compile crates/wvx-adapter-external-demo \
  -o /tmp/loom-forge-adapters --name upper --check

# Gate C pilot harness
cargo run -p wvx-cli -- forge gate-c --workspace .
# or: cargo test -p wvx-forge economics::tests::gate_c_pilot_metrics
```

## Fixture set

| Package | Function | Expected capability |
|---------|----------|---------------------|
| `wvx-adapter-external-demo` | `upper_parse` | `data.json.parse@1` |
| `wvx-adapters` | `parse` | `data.json.parse@1` |
| `wvx-adapters` | `serialize` | `data.json.serialize@1` |
| `wvx-adapters` | `path_set` | `data.json.path_set@1` |
| `wvx-component-sdk` | `register_plugin` | no JSON ontology reuse |

## Residual

- AST extract uses `syn` (static); unparseable files fall back to line heuristics.
- **ADR-0012:** local AST/inventory is **bootstrap**; deep repository intelligence belongs to **Weavatrix**. Forge should converge to *semantic classification only*.
- Compileable adapters path-depend on the scanned crate (host-local); broad packaging is **Realforge**.
- Gate C **production** Go still needs multi-domain sample + human review metrics.
- **FerroSift** is the transform-ops runtime (separate repo); not a rival capability Registry.

## Verdict

**Go (pilot harness)** — economics pipeline is instrumented and green on the JSON vertical fixture set.
