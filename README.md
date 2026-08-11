# Weavatrix Loom

**Compose systems from verified capabilities. Get ordinary Rust.**

Weavatrix Loom is a **visual semantic compiler** and registry for proven Rust
components. You work with formal *capabilities* (parse JSON, hash with BLAKE3,
compress with Gzip) rather than raw crates and free-form prompts. Each capability
can have one or more Rust implementations that share a contract, tests, and
measured characteristics. Loom turns a typed graph of those capabilities into a
normal, readable Rust workspace.

> **Status:** public `0.1` scaffold. Libraries and CLI compile; the full editor,
> playground, and registry admission pipeline are under active development.

## Why

The next abstraction is not prettier blocks or more LLM code generation. It
appears when four things are true at once:

1. **Capability is separate from implementation**
2. **Implementations are swappable** without rewriting the semantic graph
3. **Compatibility and behavior are checked**, not only documented
4. **Humans and AI edit the same structural model** (not free-form file edits)

## Core ideas

| Concept | Meaning |
| --- | --- |
| **Capability** | What must be done (stable contract, ports, errors) |
| **Implementation** | Which Rust code fulfills that contract |
| **Instance** | A capability placed in a project with config |
| **Binding** | A validated connection between typed ports |
| **Evidence** | Why an implementation can be trusted (build, conformance, benchmarks, policy) |
| **WVX** | The project / IR format (`.wvx`) |

Boundary types in `0.1` are owned and canonical (`string`, `bytes`,
`json.value`, lists, options, records, …). Upstream crates may use any Rust
types internally; adapters normalize them at the component boundary.

## Workspace

This repository is a **Rust library platform** plus thin hosts. The visual
Studio UI lives in a separate repository and talks to the same command surface.

| Crate | Role |
| --- | --- |
| `wvx-types` | Canonical boundary types |
| `wvx-ir` | WVX entities (capability, implementation, instance, binding, project) |
| `wvx-project-graph` | Project graph operations |
| `wvx-validator` | Structural and type validation |
| `wvx-runtime` | Dynamic playground execution (erased values) |
| `wvx-compiler-rust` | Export a validated graph to a native Rust workspace |
| `wvx-registry-client` | Read a local capability registry |
| `wvx-command-bus` | Shared semantic API for CLI, MCP, and future HTTP hosts |
| `wvx-cli` | Command-line entry point |
| `wvx-mcp` | Bounded MCP tools over the command bus ([`mcport`](https://crates.io/crates/mcport)) |

## Quick start

```bash
cargo check --workspace
cargo run -p wvx-cli -- --help
```

Validate and run the JSON pilot fixture:

```bash
cargo run -p wvx-cli -- validate fixtures/pilot-json-pipeline.wvx.json
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json
cargo run -p wvx-cli -- implementations
```

Swap an implementation **without** changing the capability graph or bindings:

```bash
# default: serde-json parse + serialize
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json

# alternate parse (lite recursive-descent) + pretty serialize
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json \
  --impl parse=wvx.reference.json-parse@1 \
  --impl serialize=wvx.reference.json-serialize-pretty@1
```

`run` uses built-in pilot playground handlers. Trace lines report which
implementation executed per instance.

Export a native Rust package (adapters inlined, `cargo check` / run):

```bash
cargo run -p wvx-cli -- export-rust fixtures/pilot-json-pipeline.wvx.json \
  -o /tmp/loom-export --check --run
```

The export is a normal Cargo package with `run_pipeline(&[u8]) -> Result<Vec<u8>, String>`.

## Pilot scope (`0.1`)

The first vertical slice is a pure data-transform pipeline:

```text
Input Bytes → JSON Parse → JSON Path Set → JSON Serialize → Output Bytes
```

Goals for this series:

- typed ports and validated bindings
- more than one implementation for JSON parse / serialize
- playground run with a trace
- export to a `cargo`-buildable workspace whose results match the playground
- CLI and MCP over one command bus (no free-form AI file edits)

Out of scope for `0.1`: multi-language implementations, required WASM, databases
and queues, distributed transactions, autonomous production swaps, and a full
UI builder.

## Design principles

- **Rust is the production output.** Generated code should be readable and
  maintainable outside Loom.
- **One command bus.** Studio, CLI, and MCP share the same operations; the UI is
  not a second backend.
- **Evidence is multi-fact.** Build, conformance, benchmarks, license, and
  advisories stay separate — there is no single “readiness 82%” score.
- **Fail closed on structure.** Bindings exist only after validation.
- **Transport-independent core.** Libraries do not depend on HTTP or the editor.

## Related projects

- [Weavatrix](https://github.com/sergii-ziborov/weavatrix) — repository intelligence for coding agents
- [mcport](https://crates.io/crates/mcport) — MCP stdio runtime used by `wvx-mcp`
- [blazingly-json](https://crates.io/crates/blazingly-json) — JSON engine used across the Weavatrix stack

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
