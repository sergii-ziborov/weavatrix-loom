# ADR-0007: Public evidence / private lab

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

Not every scan or failing candidate should pollute a public trust surface.

## Decision

- **Public / admitted** artifacts carry reproducible evidence (conformance, build, provenance when ready).
- **Private lab** may hold inventory-only, rejected, or experimental candidates with reasons.

v0.1 `registry-dev` is a **dev catalog**, not a full admission store. Forge inventory status `inventory_only` is not “admitted.”

## Consequences

- No global “ready %” (ADR-0008); evidence is multi-axis.
- Upstream updates create candidates; they do not auto-replace admitted impls.
