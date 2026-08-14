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

| # | Work | Why |
|---|------|-----|
| **M1** | **Truthful Registry** ([truthful-registry.md](truthful-registry.md)) | No false conformant claims; profiles + evidence artifacts + CI |
| **M2** | **Safe Semantic Core** | Validator passes, GraphPatch preview/commit, compiler policy — **landed** |
| Then | Domain 4 codecs / deeper suites | **Domain 4 pilot landed** (hex + base64 multi-impl) |

### M2 Safe Semantic Core (landed)

| Surface | What |
|---------|------|
| **Validator** | Ordered passes: schema · unique caps/ports/instances · entrypoint · bindings (types + cardinality) · cycles · impl compat · config · outputs · compiler_profile · policy. `ValidateOptions` / `ValidateOptions::release()`. |
| **GraphPatch** | `preview_patch` (ghost, no revision bump) · `commit_patch` (atomic, revision only if valid) · `base_revision` (PATCH-001). HTTP: `/graph/preview_patch`, `/graph/commit_patch`. CLI: `wvx patch preview|commit`. |
| **Compiler** | `CompilePolicy::dev()` / `::release()` — no candidates in release, trusted emit subset, SHA-256 digests (`weavatrix.digests.json`), optional `Cargo.lock`, TargetProfile + ResolveDecision explanations (`weavatrix.resolution.json`, lockfile `[resolution]`). CLI: `wvx export-rust … --release`. |

**Do not** expand domains while registry status strings overclaim.

### Domain 4 — Binary codecs (pilot)

| Capability | Multi-impl |
|------------|------------|
| `data.codec.hex_encode@1` | oneshot · chunked |
| `data.codec.hex_decode@1` | nibble · table |
| `data.codec.base64_encode@1` | crate STANDARD · pure |
| `data.codec.base64_decode@1` | crate STANDARD · pure |

```powershell
cargo run -p wvx-cli -- validate fixtures/pilot-codec-pipeline.wvx.json
cargo run -p wvx-cli -- run fixtures/pilot-codec-roundtrip.wvx.json --input-json "hello"
cargo run -p wvx-cli -- export-rust fixtures/pilot-codec-pipeline.wvx.json -o $env:TEMP\loom-codec --check
```

### Library expansion (beta+)

- Group by **family** (`data.json.*`, `data.text.*`, `io.*`)
- Filter by **kind** · sort by name / impl count
- Tab: **Capabilities | Implementations** (lifecycle + evidence browse)
- Richer cards: port type flow `bytes → json_value`
- Detail: copy key, status histogram, Forge CTA

### P2 Multi-domain Studio surface (landed)

| Surface | What |
|---------|------|
| **Pilot catalog** | HTTP `GET /api/v1/pilot/catalog` + Studio pilot menu (JSON · text · hash · compress · codec) |
| **Families / profiles** | `GET /registry/families`, `GET /registry/profiles` |
| **Trust strip** | Studio header chips → `admission` + `truthful` (read-only) |
| **Resolve / verify** | `POST /registry/resolve`, `GET /registry/verify-evidence/{id}` |

### Second capability family: `data.text.*`

| Capability | Ports | Implementations (multi-impl swap) |
|------------|-------|-------------------------------------|
| `data.text.uppercase@1` | bytes → bytes | Unicode upper · ASCII-only upper |
| `data.text.lowercase@1` | bytes → bytes | Unicode lower · ASCII-only lower |

Fixture: [`fixtures/pilot-text-pipeline.wvx.json`](../fixtures/pilot-text-pipeline.wvx.json)

```powershell
cargo run -p wvx-cli -- validate fixtures/pilot-text-pipeline.wvx.json
# --input is a file path (or - for stdin)
Set-Content $env:TEMP\t.txt 'Hello Loom' -NoNewline
cargo run -p wvx-cli -- run fixtures/pilot-text-pipeline.wvx.json --input $env:TEMP\t.txt
cargo run -p wvx-cli -- export-rust fixtures/pilot-text-pipeline.wvx.json -o $env:TEMP\loom-text --check
```

Studio: Library shows family **data.text**; drag onto canvas like JSON caps.

### Weavatrix facts → Forge (ADR-0012)

Preferred path (no product MCP; no embed of Weavatrix):

```text
Weavatrix export  →  wvx.facts.v0.1 JSON  →  Forge match/draft  →  Registry
```

| Surface | Command / API |
|---------|----------------|
| CLI load | `wvx forge facts fixtures/weavatrix-facts-sample.json` |
| CLI match | `wvx forge match --facts fixtures/weavatrix-facts-sample.json` |
| CLI draft | `wvx forge draft --facts fixtures/weavatrix-facts-sample.json` |
| Bootstrap → facts | `wvx forge export-facts <crate> -o facts.json` |
| HTTP | `POST /api/v1/forge/facts` · match/draft accept `facts` / `facts_json` / `facts_path` |

Schema: `schema_version: "wvx.facts.v0.1"` (`schemas/wvx.facts.v0.1.json`).  
Bootstrap Cargo/AST extract remains for **offline pilots only** — CLI/HTTP warn
`deprecated product path`; prefer facts. Drafts emit `Implementation.source_ref`
(`provider` + `entity_id` + optional `revision`) so Loom references Weavatrix
entities without copying the code graph (ADR-0012).
