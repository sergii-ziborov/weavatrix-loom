# ADR-0011: Gate F — Core-independent extensibility

- **Status:** Accepted (target for v0.2; pilot proof lands with SDK host)
- **Date:** 2026-08-12

## Context

v0.1 proved the JSON vertical: capability ≠ implementation, GraphPatch, playground ≡ export, registry evidence (Gates A/D/E pilot). Runtime and compiler still map **hardcoded** implementation IDs to handlers and emit paths.

That means Registry describes only what core already knows. Loom remains a strong *vertical compiler*, not a registry-driven component platform.

## Decision

**Gate F** is the next fundamental gate:

```text
Unknown external implementation
  + SDK adapter (declared ABI)
  + Registry manifest
  + conformance evidence
      ↓
Studio discovers it
Playground executes it
Compiler exports it
Core pilot match tables are unchanged
```

### Rules

1. **No new pilot `match` arms** required for a new external adapter.  
2. Adapters conform to **`wvx-component-sdk`** (manifest + runtime register + emit descriptor).  
3. **Host may wire** plugin registration once (e.g. call `register()` from an optional host list / env); core `with_pilot()` tables stay pilot-only.  
4. Compiler uses **manifest-driven emit** (`sdk.emit`) when present; falls back to legacy pilot map only for v0.1 IDs.  
5. AI suggestions and Forge drafts remain non-evidence (ADR-0010 / 0007).

### Non-goals (later gates)

- Dynamic `.dll` loading without rebuild  
- Full Wasm component model  
- Production public registry trust fleet (Gate G)

## Consequences

- New crate: `wvx-component-sdk`  
- External fixture adapter proves Gate F  
- Full product roadmaps stay **private** (`private/`, gitignored); this ADR is the public architectural commitment  
- Studio remains the composition shell for **any** domain once Gate F holds — not “JSON-only”

## Related

- ADR-0002 Capability / Implementation / Instance  
- ADR-0005 Dynamic runtime vs static compiler  
- Go/No-Go A/D, E pilot notes under `docs/`
