# ADR-0006: Optional Wasm boundary

- **Status:** Accepted
- **Date:** 2026-08-12
- **Updated:** 2026-08-18

## Context

Wasm sandboxes are useful later; WASI/async support is still evolving. Forcing Wasm would block native crates (SIMD JSON, rayon).

## Decision

Schema and product vision **may** include Wasm components later. The v0.1 **production path is native Rust only** (`export-rust` / `compile_release`).

Wasm is an **optional sidecar + thin host**:

- `wvx export-wasm` / `export_wasm_to_directory` emit a Cargo package with
  `.cargo/config.toml` targeting `wasm32-wasip1`.
- `wvx run-wasm` / `run_wasm_in_directory` builds that package and runs it
  with the **wasmtime CLI** on PATH (preopen `.`, `WVX_PIPELINE_INPUT_FILE`).
- Native-only adapters (`simd-json.parse@1`, `sonic-rs.parse@1`,
  `blake3.blake3-parallel@1`) are **rejected** at export time.
- The vendored `wvx-adapters` crate on this path has no simd-json / sonic-rs /
  blake3-rayon features.
- Missing `wasm32-wasip1` rustup target or missing `wasmtime` is **fail-closed**.
  Loom does not install them and does not embed cranelift.

Wasm is **not** required for admission of pilot components.

## Consequences

- No **embedded** Wasm VM and no WIT component model in 0.1.
- Host = invoke `wasmtime` (same artifact a user can run by hand).
- FerroSift remains the Wasm-capable transform runtime in the ecosystem.
- Future: WIT/export as a separate profile, not a rewrite of the IR core.
