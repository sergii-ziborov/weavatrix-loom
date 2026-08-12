# Go / No-Go notes — Gate F (Core-independent extensibility)

**Date:** 2026-08-12  
**Scope:** External SDK adapter not listed in pilot match tables  
**ADR:** [0011-gate-f-core-independent-extensibility.md](adr/0011-gate-f-core-independent-extensibility.md)

| Gate | Title | Pilot verdict |
|------|--------|---------------|
| **F** | Core-independent extensibility | **Go (pilot fixture)** |

## Criterion

Unknown external implementation + SDK adapter + Registry manifest → Studio/CLI can run and export **without** new arms in `with_pilot()` / legacy compiler match tables.

## Fixture

| Piece | Location |
|-------|----------|
| SDK ABI | `crates/wvx-component-sdk` |
| External adapter | `crates/wvx-adapter-external-demo` (`upper_parse`) |
| Registry manifest | `registry-dev/implementations/external.demo.upper-parse@1.json` with `sdk.emit` |
| Host wire | `wvx_adapter_external_demo::register()` + `install_plugins` in command-bus host |

## Evidence

```bash
# Runtime (playground) — not in pilot match tables
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=external.demo.upper-parse@1
# → string leaves uppercased, e.g. "hello" → "HELLO"

# Static export uses sdk.emit template + vendored crate
cargo run -p wvx-cli -- export-rust fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=external.demo.upper-parse@1 -o /tmp/loom-gate-f --check --run
```

## Residual

- Host still **calls** `register()` once (acceptable per ADR-0011); no pilot match arm for the external id.
- Full dynamic `.dll` discovery and multi-domain packs remain future work.
- External demo is intentionally **not** identity-parse conformant (candidate status).

## Verdict

**Go (pilot fixture)** — registry-driven SDK path works for one external parse adapter without core pilot tables knowing its ID.
