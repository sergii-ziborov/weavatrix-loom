# ADR-0004: GraphPatch is the edit model

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Free-form file edits by humans or AI bypass validation and destroy interchangeability guarantees.

## Decision

Structured edits (Studio, CLI, MCP, future AI) use **GraphPatch** ops (`add_instance`, `connect`, `select_implementation`, …). Apply goes through Graph Core validation. Propose may be rule-based or LLM-generated; **only validated apply is authoritative**.

Studio does **not** use MCP as an internal bus; MCP is an external adapter over the same ops.

## Consequences

- AI must emit ops, not arbitrary diffs (ADR-0010).
- Relative propose (pilot recipe vs current project) is allowed; still applied as a patch.
