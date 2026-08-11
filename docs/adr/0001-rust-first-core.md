# ADR-0001: Rust-first core

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Loom needs one language for IR, validation, playground, compiler, Forge, and hosts so semantic rules are not reimplemented in TypeScript.

## Decision

Core libraries, runtime, compiler, Forge, CLI, MCP, and HTTP server are **Rust**. Studio is **TypeScript** and talks JSON over the command bus / HTTP — never embeds graph rules.

## Consequences

- Single semantic implementation surface (`wvx-command-bus`).
- Studio stays thin; offline/invalid UI state is local preview only until server validates.
- Wasm and other languages may appear later as **targets/adapters**, not as alternate cores.
