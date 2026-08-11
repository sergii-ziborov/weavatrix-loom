# ADR-0006: Optional Wasm boundary

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Wasm sandboxes are useful later; WASI/async support is still evolving. Forcing Wasm would block native crates.

## Decision

Schema and product vision **may** include Wasm components later. v0.1 production path is **native Rust only**. Wasm is optional, not required for admission of pilot components.

## Consequences

- No Wasm host in 0.1.
- Future: WIT/export as a separate profile, not a rewrite of the IR core.
