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
