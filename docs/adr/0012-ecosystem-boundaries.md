# ADR-0012: Ecosystem boundaries — Weavatrix / Loom / Realforge / FerroSift / Cortex

- **Status:** Accepted
- **Date:** 2026-08-13

## Context

Several products share words like *graph*, *registry*, *forge*, and *loom*.
Without hard boundaries we risk:

- a second code indexer inside Weavatrix Loom;
- Registry or semantic compile leaking into packaging tools;
- FerroSift and Loom both claiming “operation registry” without a clear link;
- Cortex owning truth instead of agent workflow.

Target pipeline:

```text
UNDERSTAND → SEMANTIC → CONSTRUCT
 Weavatrix      Loom      Realforge
```

AI control plane (Cortex, GrantTap, …) sits **above**, not between truth layers.

## Decision

### Product monopoly (one question → one owner)

| Product | Repo (canonical) | Owns the question | Must not own |
|---------|------------------|-------------------|--------------|
| **Weavatrix** | `weavatrix`, `weavatrix-graph`, … | What exists in **code** (facts, symbols, deps, search, impact)? | Capability interchange registry; WVX project graph |
| **Weavatrix Loom** | `weavatrix-loom`, `loom-studio` | What does it **mean**, what is **proven**, how to **compose** → Rust? | Deep repo indexing; generic deploy packaging |
| **Realforge** | (construct product; name TBD vs ReelForge) | How to **build/scaffold/package/deploy** artifacts? | Capability admit policy; code symbol graph |
| **FerroSift** | `ferrosift` | How to run **deterministic transform recipes** (ops runtime)? | Code intelligence; WVX capability registry |
| **Cortex** | `cortex-loom` (+ thin `wvx-cortex` bridge) | How agents **select context / route / workflow**? | Evidence pass; registry admit; IR authority |
| **GrantTap / control** | as applicable | Agent **control plane** | Semantic IR; code facts |

### Loom core (stays in Loom — unique)

```text
Capability → Implementation → Conformance → Evidence → Resolution → Compile → Rust
```

- WVX IR (Capability, Implementation, Instance, Binding, contracts)
- Project graph + **GraphPatch**
- Validator + playground runtime
- **Registry** (interchangeable implementations + multi-fact evidence)
- **Semantic compiler** (resolve → specialize → generate ordinary Rust)
- Conformance / gate harnesses

### Loom Forge (repositioned)

**Not** a second Weavatrix. Target shape:

```text
Weavatrix (code facts)
        │
        ▼
Loom Forge  — thin semantic ingestion / classification
        │
        ▼
Loom Registry — draft → candidate → conformant → admit (human/policy)
```

Example:

- Weavatrix: `pub fn parse(input: &[u8]) -> Result<Value, E>` (+ span, revision)
- Loom Forge: *candidate for* `data.json.parse@1`
- Registry: lifecycle + evidence axes (never AI-only pass — ADR-0010)

**Transitional:** local Cargo/AST inventory in `wvx-forge` is **bootstrap only** until Weavatrix feeds facts. It must not be positioned as Loom’s product code-graph.

### Compiler vs Realforge

| Loom compiler | Realforge |
|---------------|-----------|
| Capability graph → resolved impls → pipeline Rust | Workspace scaffolding, CI, multi-package products, deploy |
| May vendor adapters for *this* composition | Broader packaging and operational workflows |
| Owns semantic IR | **Calls** Loom compile API; does not own WVX |

### FerroSift relationship

FerroSift is a **portable ops/recipe runtime** (deterministic transforms, CyberChef-compatible aliases where declared).

- **Not** a rival Registry of Loom capabilities.
- **Not** a code indexer.
- Natural fit: **implementations** (or recipe-backed adapters) that *fulfill* Loom capabilities, and/or construction-time recipes Realforge may package.
- Link direction: FerroSift op → Loom Implementation (optional), never Loom embedding FerroSift’s full op registry as WVX IR.

### Graph rule (anti-chaos)

**No unified graph** across products.

| Graph | Owner |
|-------|--------|
| Code graph (File, Fn, Call, Import, …) | Weavatrix |
| Semantic / capability graph (Capability, Instance, Binding, …) | Loom |
| Recipe / op graph (steps, budgets) | FerroSift |
| Agent / process graph | Cortex |

Loom **references** Weavatrix entities via provenance (`source_ref`), it does **not** copy the code graph.

### Naming

- Architecture text: **Weavatrix Loom** (full name).
- “Loom” alone = local shorthand for Weavatrix Loom only, never Weavatrix or Cortex Loom.
- “Forge” in Studio = **Loom Forge** (ingestion), distinct from **Realforge** (construct).

## Consequences

1. New deep AST / index / symbol search → Weavatrix, not `wvx-forge` growth.
2. New packaging / scaffold product features → Realforge, not Loom core.
3. New transform ops libraries → FerroSift (or adapters into Loom Implementations).
4. README and Studio copy must not call Loom a “code graph” product.
5. Migration of bootstrap extract → Weavatrix API is planned; until then mark bootstrap paths clearly.

## Related

- ADR-0002 Capability / Implementation / Instance  
- ADR-0005 Dynamic runtime vs static compiler  
- ADR-0007 / 0010 Evidence and AI  
- ADR-0011 Gate F  
- [`docs/ecosystem-distribution.md`](../ecosystem-distribution.md)
