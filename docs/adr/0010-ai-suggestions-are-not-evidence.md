# ADR-0010: AI suggestions are not evidence

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

LLMs can propose graphs and adapters; that must not substitute for conformance or review.

## Decision

AI may propose GraphPatch ops or Forge candidates. Suggestions are **ghost** until validated apply. They never count as evidence for admission. Conformance, build, benchmarks, and human review remain the trust path.

## Consequences

- MCP tools stay bounded (validate, run, patch, registry, forge inventory).
- “LLM said so” is not an evidence axis in UI or registry.
