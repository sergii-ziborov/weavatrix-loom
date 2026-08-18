# ADR-0006: Optional Wasm boundary

- **Status:** Accepted
- **Date:** 2026-08-12
- **Updated:** 2026-08-18

## Context

Wasm sandboxes are useful later; WASI/async support is still evolving. Forcing Wasm would block native crates (SIMD JSON, rayon).

## Decision

Schema and product vision **may** include Wasm components later. The v0.1 **production path is native Rust only** (`export-rust` / `compile_release`).

Wasm is an **optional sidecar**:

- `wvx export-wasm` / `export_wasm_to_directory` emit a Cargo package with
  `.cargo/config.toml` targeting `wasm32-wasip1`.
- Native-only adapters (`simd-json.parse@1`, `sonic-rs.parse@1`,
  `blake3.blake3-parallel@1`) are **rejected** at export time.
- The vendored `wvx-adapters` crate on this path has no simd-json / sonic-rs /
  blake3-rayon features.
- `--check` runs `cargo check --target wasm32-wasip1` when the rustup target is
  installed; it does not install the target.

Wasm is **not** required for admission of pilot components.

## Consequences

- No Wasm **host**, wasmtime runtime, or WIT component model in 0.1.
- FerroSift remains the Wasm-capable transform runtime in the ecosystem.
- Future: WIT/export as a separate profile, not a rewrite of the IR core.
