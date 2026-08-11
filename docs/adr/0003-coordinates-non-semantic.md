# ADR-0003: Canvas coordinates are non-semantic

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Users need free layout. Layout must not affect compile/run meaning.

## Decision

`instance.ui` (`x`, `y`) is **presentation only**. Compiler, runtime, validator, and export ignore coordinates. Auto-layout and drag never change bindings or configs.

## Consequences

- Safe free placement and multi-view layout later.
- “Staged” vs “bound” is about bindings/reachability, not position.
