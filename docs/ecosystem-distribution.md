# Ecosystem distribution (quick map)

**Date:** 2026-08-13  
**Normative ADR:** [ADR-0012](adr/0012-ecosystem-boundaries.md)

One question → one product. No dual code graph. No dual capability registry.

```text
                         AI
                          │
              ┌───────────┴───────────┐
              │                       │
          Cortex                 GrantTap / …
       token economy             agent control
              │
              ▼
       ┌───────────────┐
       │   Weavatrix   │  UNDERSTAND — code facts
       └───────┬───────┘
               │
               ▼
       ┌───────────────┐
       │ Weavatrix Loom│  SEMANTIC — compose + prove + compile
       │  (Forge thin) │
       └───────┬───────┘
               │ capability graph / resolved export
               ▼
       ┌───────────────┐
       │   Realforge   │  CONSTRUCT — artifacts / package / deploy
       └───────────────┘

FerroSift ──► optional Implementations / recipes (not a second Registry)
```

## Ownership matrix

| Concern | Weavatrix | **Loom** | Realforge | FerroSift | Cortex |
|---------|:---------:|:--------:|:---------:|:---------:|:------:|
| Repo index, symbols, call/import graph | **Y** | — | — | — | — |
| Deep Rust AST / search / impact | **Y** | — | — | — | — |
| Capability / Implementation / Instance / Binding | — | **Y** | — | — | — |
| GraphPatch, validator, playground | — | **Y** | — | — | — |
| **Registry** (interchange + evidence + resolve) | — | **Y** | — | — | — |
| **Semantic compiler** → ordinary Rust | — | **Y** | call | — | — |
| Conformance / bench gates (capability axes) | — | **Y** | — | — | — |
| Thin semantic ingestion (Forge) | facts in | **Y** | — | — | — |
| Workspace scaffold, CI, deploy packages | — | — | **Y** | — | — |
| Deterministic transform ops / recipes | — | adapt | package | **Y** | — |
| Context selection, model routing, agent flow | — | bridge | — | — | **Y** |

**Y** = owns. **call** = may invoke API. **adapt** = may wrap as Implementation. **bridge** = thin host (e.g. intent→GraphPatch), not full Cortex product. **facts in** = Weavatrix supplies facts to Loom Forge.

## What moves out of Loom (direction, not big-bang)

| Current Loom piece | Target owner | Notes |
|--------------------|--------------|--------|
| Deep Cargo/AST as *product* | **Weavatrix** | `wvx-forge` extract = **bootstrap only** until API feed |
| Generic multi-crate scaffold / deploy | **Realforge** | Loom keeps *semantic* export of one composition |
| Transform op libraries (hash/encode/…) | **FerroSift** → Loom Implementations | FerroSift is runtime; Loom registers proven adapters |
| Full agent orchestration | **Cortex** (`cortex-loom`) | `wvx-cortex` stays minimal GraphPatch propose |

## What stays in Loom (do not move)

- WVX IR + schemas  
- Registry lifecycle + multi-fact evidence  
- Resolver + semantic compiler  
- GraphPatch + command bus  
- Playground runtime + Gate A/D harness  
- Gate F SDK host (register plugins, not code index)  
- Studio as composition shell  

## FerroSift (not lost)

| | |
|--|--|
| **Is** | Pure-Rust deterministic recipe/ops runtime (local-first, Wasm-capable) |
| **Is not** | Weavatrix indexer · Loom capability Registry · Realforge packaging product |
| **Links to Loom** | Op or recipe profile can back an **Implementation** of a Capability (after conformance) |
| **Links to Realforge** | Recipes may be packaged into larger artifacts |
| **Links to Weavatrix** | Optional: map op source to code entities for provenance |

## Loom Forge (target)

```text
Weavatrix facts  →  classify / match  →  draft Implementation  →  Registry
```

Not: build a second symbol graph inside Loom.

## Naming cheat-sheet

| Say | Mean |
|-----|------|
| Weavatrix | Code intelligence |
| Weavatrix Loom / WVX | Semantic composition product |
| Loom Studio | UI over Loom |
| Loom Forge | Semantic ingestion stage (thin) |
| Realforge | Artifact construction |
| FerroSift | Transform recipe runtime |
| Cortex Loom | Agent process / token economy |

## Status of code (2026-08-13)

| Area | State |
|------|--------|
| Loom Registry + compiler | In-tree, keep |
| `wvx-forge` inventory/AST | Bootstrap; labeled transitional |
| Weavatrix → Forge API | Not wired yet (planned) |
| Realforge product crate | External / TBD — Loom does not implement it |
| FerroSift | Separate repo; no ownership claim by Loom |
| `wvx-cortex` | Thin intent→GraphPatch only |
