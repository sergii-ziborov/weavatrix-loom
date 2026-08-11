# ADR-0008: No global readiness percentage

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

A single “82% ready” score hides missing security review, missing benchmarks, or license gaps.

## Decision

Do not expose a single readiness percentage. Show independent evidence axes (build, conformance, benchmark, license, security, …) and discrete lifecycle labels (e.g. inventory_only, candidate, conformant, admitted).

## Consequences

- UI chips/lists per fact, not one score.
- Go/No-Go gates are narrative + harness, not a number.
