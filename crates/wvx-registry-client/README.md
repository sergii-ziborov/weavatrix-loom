# `wvx-registry-client`

Local filesystem client for a Loom capability registry (lifecycle + multi-fact evidence,
HMAC attestations, offline Sigstore-shaped in-toto/DSSE bundles, local
hashedrekord, SPDX 2.3 JSON — not Fulcio, not public Rekor, not SPDX 3.0).

Part of **[Weavatrix Loom](https://github.com/sergii-ziborov/weavatrix-loom)** — semantic software composition
(Capability · Implementation · Registry · GraphPatch · compile to ordinary Rust).

> **Status:** v0.1 beta libraries. APIs may evolve before 1.0.

## Install

```toml
[dependencies]
wvx-registry-client = "0.1"
```

Or path-depend from a sibling checkout of the monorepo (still fully supported):

```toml
wvx-registry-client = { path = "../weavatrix-loom/crates/wvx-registry-client" }
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
use wvx_registry_client::LocalRegistry;
```

See the monorepo README and `docs/crate-surface.md` for the full crate map and host entry points.

## License

MIT OR Apache-2.0
