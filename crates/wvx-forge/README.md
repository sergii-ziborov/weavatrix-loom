# `wvx-forge`

Thin Loom Forge: Weavatrix facts / bootstrap extract → match → draft (not a code indexer).

Part of **[Weavatrix Loom](https://github.com/sergii-ziborov/weavatrix-loom)** — semantic software composition
(Capability · Implementation · Registry · GraphPatch · compile to ordinary Rust).

> **Status:** v0.1 beta libraries. APIs may evolve before 1.0.

## Install

```toml
[dependencies]
wvx-forge = "0.1"
```

Or path-depend from a sibling checkout of the monorepo (still fully supported):

```toml
wvx-forge = { path = "../weavatrix-loom/crates/wvx-forge" }
```

## Role in the stack

| Layer | Product |
| --- | --- |
| Code facts | [Weavatrix](https://github.com/sergii-ziborov/weavatrix) (UNDERSTAND) |
| Semantic composition | **Weavatrix Loom** (this crate family) |
| UI | [Loom Studio](https://github.com/sergii-ziborov/loom-studio) → HTTP `loom-server` only |
| Agent MCP | Optional hosts only — **not** product wiring |

Normative boundaries: [ADR-0012](https://github.com/sergii-ziborov/weavatrix-loom/blob/main/docs/adr/0012-ecosystem-boundaries.md).

## Example

```rust
use wvx_forge::{extract_from_facts, match_candidates};
```

See the monorepo README and `docs/crate-surface.md` for the full crate map and host entry points.

## License

MIT OR Apache-2.0
