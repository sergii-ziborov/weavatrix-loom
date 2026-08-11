# ADR-0002: Capability / Implementation / Instance split

- **Status:** Accepted
- **Date:** 2026-08-12

## Context

If the canvas binds users to crate names, swapping backends rewrites the graph and breaks “compose verified capabilities.”

## Decision

Always separate:

| Entity | Meaning |
|--------|---------|
| **Capability** | What (contract, ports, errors, effects) |
| **Implementation** | How (crate/adapter id fulfilling a capability) |
| **Instance** | Placement of a capability in a project (config, optional impl choice, UI) |

Bindings connect **instances/ports**, never crates.

## Consequences

- Impl swap is an instance field (or GraphPatch `select_implementation`), not a rewire.
- Registry indexes both capabilities and implementations.
- Gate A (interchangeability) is meaningful only under this split.
