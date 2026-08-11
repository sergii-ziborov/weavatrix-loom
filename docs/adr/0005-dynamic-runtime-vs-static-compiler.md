# ADR-0005: Dynamic runtime vs static compiler

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Studio needs instant Run; production needs ordinary Rust without Loom.

## Decision

Two paths share the same WVX graph:

1. **Playground** — erased `WvxValue` handlers (`wvx-runtime`), fast feedback.
2. **Compiler** — generate a Cargo package with typed adapter calls (`wvx-compiler-rust`).

Semantic equivalence on the pilot is Gate D evidence (JSON value equality), not bit-identical bytes.

## Consequences

- Adapters must be dual-registered (runtime handler + emit map) when added.
- Dynamic dispatch stays out of the hot static path by design.
