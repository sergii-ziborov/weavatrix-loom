# Go / No-Go notes — Gate F (Core-independent extensibility)

**Date:** 2026-08-12  
**Scope:** External SDK adapter + full pilot transform migration onto SDK  
**ADR:** [0011-gate-f-core-independent-extensibility.md](adr/0011-gate-f-core-independent-extensibility.md)

| Gate | Title | Pilot verdict |
|------|--------|---------------|
| **F** | Core-independent extensibility | **Go (pilot + SDK migration)** |

## Criterion

Unknown external implementation + SDK adapter + Registry manifest → Studio/CLI can run and export **without** new arms in `with_pilot()` / legacy compiler match tables.

Additionally (v0.2 ABI close-out): **all pilot transforms** register via the same SDK path; `with_pilot()` retains **I/O only**.

## Fixture

| Piece | Location |
|-------|----------|
| SDK ABI | `crates/wvx-component-sdk` |
| Pilot transforms | `wvx_adapters::register_pilot_plugins()` (feature `host`) |
| External adapter | `crates/wvx-adapter-external-demo` (`parse`) |
| Registry manifests | `registry-dev/implementations/*` with `sdk.emit` (parse/serialize) |
| Host wire | command-bus `playground_handlers()` → pilot plugins + external `register()` + `install_plugins` |

## Evidence

```bash
# Runtime (playground) — not in pilot match tables
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=external.demo.json-parse@1
# → identity parse (semantically equivalent), e.g. "hello" stays "hello"

# Static export uses sdk.emit template + vendored crate
cargo run -p wvx-cli -- export-rust fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=external.demo.json-parse@1 -o /tmp/loom-gate-f --check --run

# Pilot transforms also via SDK (default pipeline)
cargo test -p wvx-conformance --lib
```

## Residual

- Host still **calls** `register()` / `register_pilot_plugins()` once (acceptable per ADR-0011).
- `path_set` static emit still special-cases config inlining (runtime uses SDK `path_set_handler`).
- Full dynamic `.dll` discovery and multi-domain packs remain future work.
- External demo is a semantically equivalent parser (`json-rfc8259-core-v1`); still **candidate** until a suite artifact exists.

## Verdict

**Go** — external fixture + pilot transforms on SDK ABI; core runtime no longer owns transform match tables.
