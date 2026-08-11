# ADR-0009: Export without platform lock-in

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Professional users will not adopt a tool that traps code inside a proprietary runtime.

## Decision

`export-rust` produces a **normal Cargo package** that builds with the standard toolchain without Loom Online. Pilot adapters are vendored (`vendor/wvx-adapters`) so the export is self-contained.

## Consequences

- Escape hatch is first-class (Code mode / download export).
- Generated code should stay readable; magic runtimes are rejected.
