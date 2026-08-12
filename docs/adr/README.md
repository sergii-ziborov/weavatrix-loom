# Architecture Decision Records

Short, irreversible (or costly-to-reverse) decisions for Weavatrix Loom v0.1+.

| ID | Title | Status |
|----|--------|--------|
| [ADR-0001](0001-rust-first-core.md) | Rust-first core | Accepted |
| [ADR-0002](0002-capability-implementation-instance.md) | Capability / Implementation / Instance split | Accepted |
| [ADR-0003](0003-coordinates-non-semantic.md) | Canvas coordinates are non-semantic | Accepted |
| [ADR-0004](0004-graphpatch-authoritative-edits.md) | GraphPatch is the edit model | Accepted |
| [ADR-0005](0005-dynamic-runtime-vs-static-compiler.md) | Dynamic runtime vs static compiler | Accepted |
| [ADR-0006](0006-optional-wasm-boundary.md) | Optional Wasm boundary | Accepted |
| [ADR-0007](0007-public-evidence-private-lab.md) | Public evidence / private lab | Accepted |
| [ADR-0008](0008-no-global-readiness-percentage.md) | No global readiness % | Accepted |
| [ADR-0009](0009-export-without-lock-in.md) | Export without platform lock-in | Accepted |
| [ADR-0010](0010-ai-suggestions-are-not-evidence.md) | AI suggestions are not evidence | Accepted |
| [ADR-0011](0011-gate-f-core-independent-extensibility.md) | Gate F — core-independent extensibility (v0.2) | Accepted |

**Format truth:** Rust types in `wvx-ir` / `wvx-types` remain authoritative; JSON Schemas under [`schemas/`](../../schemas/) describe the wire shape for tooling and Studio.

**Related:** [Go/No-Go A & D (JSON pilot)](../go-no-go-a-d-pilot-json.md) · [Gate E pilot](../go-no-go-e-pilot.md)

**Private product roadmaps** live under `private/` / `plan/` (gitignored) — never committed.
