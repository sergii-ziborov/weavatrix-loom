# Weavatrix Loom — Beta (v0.2 track)

**Status:** **beta** local product slice  
**Date:** 2026-08-13  
**Supersedes maturity label:** alpha docs still valid for smoke scripts; this doc is product intent.

## Beta promise

A developer can:

1. Compose a **capability graph** from the **Registry Library** (not Cargo rows).
2. **Ingest any crate/workspace path** via **Forge** (inventory all members → match → draft).
3. **Validate · run · multi-impl swap · export Rust** through Studio (HTTP) or CLI.
4. Understand boundaries: Weavatrix = facts, Loom = semantic, Realforge = construct.

Not promised: production admit fleet, hosted registry, crates.io auto-install into Library, product MCP.

## Library vs packages (beta rule)

| Surface | Contains | How it grows |
|---------|----------|--------------|
| **Library** | Capabilities + listed Implementations | Registry content + drafts that are registered |
| **Forge** | Cargo path / workspace inventory | User points absolute path on loom-server host |

```text
Any package path  →  Forge  →  draft Implementation  →  Registry  →  Library
```

## Studio UX (beta)

- **About** (`?`) — product map, versions, shortcuts, start commands  
- Mode hints: Design / Run / Code / Forge  
- Library banner: capabilities only + **Open Forge**  
- Forge copy: add package source, never admits  
- Chip: `beta` · loom-server · nodes  

### README screenshots

Live UI captures (English UI) live under `docs/images/` and are mirrored in
[loom-studio](https://github.com/sergii-ziborov/loom-studio):

| File | Surface |
|------|---------|
| `studio-design.png` / `studio-pilot.png` | Design · Library + canvas |
| `studio-inspector.png` | Instance inspector · multi-impl |
| `studio-run.png` | Run · trace + output |
| `studio-library.png` | Capability detail page |
| `studio-forge.png` | Forge wizard |
| `studio-about.png` | About modal |

Refresh from Studio: `node scripts/capture-screens.mjs` (Edge/Chrome + stack up).

## Smoke

```powershell
cd weavatrix-loom
powershell -File ./scripts/alpha-smoke.ps1   # CLI + HTTP still valid
cd ../loom-studio
npm run alpha:check   # test + build (+ check:api if server up)
```

## Maturity ladder

| Label | Meaning |
|-------|---------|
| alpha | First working vertical |
| **beta** | Clear product surfaces + path ingest + Studio About/UX |
| rc / 1.0 | Weavatrix facts feed, second capability family, publish policy |

## What next (priority order)

Product focus after Library/Forge beta surfaces:

| # | Work | Why |
|---|------|-----|
| **P0** | **Library catalog UX** (families, kind filter, Implementations tab) | Registry is the product surface — must scale past 5 pilot caps |
| **P1** | **Second capability family** (beyond JSON transform) | Proves Registry is not a one-off vertical |
| **P1** | **Weavatrix facts → Forge** | Replace bootstrap Cargo/AST as product story (ADR-0012) |
| **P2** | **crates.io library crates** | Path deps first; flip `publish` only after README + API freeze |
| **P2** | **Capability `description` / tags** in IR + registry | Human browse without opening every detail page |
| **P3** | Hosted registry / admit fleet | Not beta; needs evidence + policy product |
| **Never product** | Studio/MCP product wiring | MCP stays agent-only; Studio stays HTTP → loom-server |

**Do not split monorepo into crate sub-repos** until public API freeze — path workspace is the consumer mode.

### Library expansion (beta+)

- Group by **family** (`data.json.*`, `io.*`)
- Filter by **kind** · sort by name / impl count
- Tab: **Capabilities | Implementations** (lifecycle + evidence browse)
- Richer cards: port type flow `bytes → json_value`
- Detail: copy key, status histogram, Forge CTA
