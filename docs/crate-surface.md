# Loom crate surface (consumer access)

**Date:** 2026-08-13  
**Normative:** [ADR-0012](adr/0012-ecosystem-boundaries.md)

## What “access to all packages” means

The monorepo is one Cargo workspace. **Every library crate is a first-class path
dependency** for sibling products (Realforge, Cortex, future hosts). There is no
hidden subset — if it is a `lib` under `crates/`, consumers may depend on it via
`path` (or later crates.io when we flip `publish`).

`publish = false` on the workspace means **not on crates.io yet**, not “private
to Loom only”. Path/git access is the supported consumer mode for v0.1.

## Product hosts vs agent adapters

| Kind | Crates | Who uses it |
|------|--------|-------------|
| **Library surface** | `wvx-types`, `wvx-ir`, `wvx-project-graph`, `wvx-validator`, `wvx-runtime`, `wvx-compiler-rust`, `wvx-registry-client`, `wvx-command-bus`, `wvx-component-sdk`, `wvx-forge`, `wvx-conformance`, `wvx-cortex`, `wvx-adapters` | Realforge / Cortex / any Rust host **links crates** |
| **Product hosts** | `loom-server` (HTTP for Studio), `wvx-cli` | Humans + Studio |
| **Agent adapter only** | `wvx-mcp` | Coding agents (stdio MCP) — **not** Studio, not Realforge |
| **Demo / fixture** | `wvx-adapter-external-demo` | Gate F demos only |

### Hard rule (no product MCP wiring)

```text
Studio  ──HTTP──►  loom-server  ──►  wvx-command-bus  ──►  libs
CLI     ─────────►  wvx-command-bus  ──►  libs
Agent   ──MCP────►  wvx-mcp       ──►  wvx-command-bus  ──►  libs   (optional)

Weavatrix MCP is a separate agent tool (UNDERSTAND). Loom and Studio never
embed or require Weavatrix or Loom MCP as a product dependency.
```

Studio talks **only** `loom-server` JSON/HTTP. Same ops as CLI/MCP, different
transport (ADR-0004).

## Path dependency example (all public libs)

From a sibling repo (e.g. Realforge or Cortex):

```toml
[dependencies]
wvx-types = { path = "../weavatrix-loom/crates/wvx-types" }
wvx-ir = { path = "../weavatrix-loom/crates/wvx-ir" }
wvx-project-graph = { path = "../weavatrix-loom/crates/wvx-project-graph" }
wvx-validator = { path = "../weavatrix-loom/crates/wvx-validator" }
wvx-runtime = { path = "../weavatrix-loom/crates/wvx-runtime" }
wvx-compiler-rust = { path = "../weavatrix-loom/crates/wvx-compiler-rust" }
wvx-registry-client = { path = "../weavatrix-loom/crates/wvx-registry-client" }
wvx-command-bus = { path = "../weavatrix-loom/crates/wvx-command-bus" }
wvx-component-sdk = { path = "../weavatrix-loom/crates/wvx-component-sdk" }
wvx-forge = { path = "../weavatrix-loom/crates/wvx-forge" }
wvx-conformance = { path = "../weavatrix-loom/crates/wvx-conformance" }
wvx-cortex = { path = "../weavatrix-loom/crates/wvx-cortex" }
wvx-adapters = { path = "../weavatrix-loom/crates/wvx-adapters" }
```

Prefer **`wvx-command-bus`** when you need the full semantic surface (validate,
run, export, registry, forge). Prefer leaf crates when you only need types/IR.

## Roles (one line each)

| Crate | Role |
|-------|------|
| `wvx-types` | Boundary types |
| `wvx-ir` | Capability / Implementation / Instance / Binding / Project |
| `wvx-project-graph` | GraphPatch apply |
| `wvx-validator` | Structural/type validation |
| `wvx-runtime` | Playground |
| `wvx-compiler-rust` | Semantic compile → ordinary Rust |
| `wvx-registry-client` | Local registry + evidence lifecycle |
| `wvx-command-bus` | Single API for all hosts |
| `wvx-component-sdk` | Gate F adapter ABI |
| `wvx-forge` | Thin semantic ingestion (bootstrap inventory/AST until Weavatrix facts) |
| `wvx-conformance` | Pilot vectors / golden |
| `wvx-cortex` | Intent → GraphPatch only (not full Cortex product) |
| `wvx-adapters` | Pilot JSON adapters |
| `wvx-cli` / `loom-server` | Product entrypoints |
| `wvx-mcp` | Agent-only MCP adapter |

## Known intentional clones

Type-1 pairs between `wvx-adapters` and `wvx-compiler-rust/src/adapter_sources/`
are **vendored export sources** (static compile embeds adapters). Do not “dedupe”
by making the compiler depend on the dynamic adapter crate at export time without
an explicit design change.

## crates.io later

When ready: flip `publish` on **library** crates only; keep hosts/demos
`publish = false` unless intentionally released as binaries.
