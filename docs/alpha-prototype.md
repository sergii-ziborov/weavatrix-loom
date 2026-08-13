# Weavatrix Loom — Alpha prototype (v0.1)

**Status:** local **alpha** for the JSON pipeline vertical.  
**Date:** 2026-08-13

This is a **working prototype / early alpha**: one proven vertical (JSON parse →
path_set → serialize), multi-impl swap, playground ≡ static export, Studio UI
over HTTP, thin Forge ingestion. It is **not** production trust (Gate E), not a
hosted registry, and not a second Weavatrix code graph ([ADR-0012](adr/0012-ecosystem-boundaries.md)).

## What works (alpha acceptance)

| Capability | How to prove |
|------------|----------------|
| Validate pilot project | `wvx validate fixtures/pilot-json-pipeline.wvx.json` |
| Run playground (multi-impl) | `wvx run …` / Studio **Run** |
| Export ordinary Rust + `cargo check` | `wvx export-rust … -o DIR --check` |
| Local registry search / inspect / overclaim check | `wvx registry …` |
| HTTP API for Studio | `loom-server` on `127.0.0.1:43917` |
| Studio design / run / code / forge | `loom-studio` → `npm run dev` (**HTTP only**) |
| Forge inventory of **all** workspace crates | Studio Forge or `POST /api/v1/forge/inventory` |
| Conformance pilot (A/D) | `wvx conformance` / CI gates job |
| Architecture health | `.weavatrix/architecture.json` + Weavatrix verify |

Automated: `powershell -File ./scripts/alpha-smoke.ps1`

## What is intentionally out of alpha

- Weavatrix → Forge live facts feed (bootstrap AST remains)
- Production multi-tenant registry / admit fleet
- Realforge packaging product
- Product MCP wiring (Studio never uses MCP; agents may use optional `wvx-mcp`)
- Non-JSON capability families (hash/hex) as first-class pilots

## Two-terminal start

**Terminal A — API**

```powershell
cd weavatrix-loom
powershell -File ./scripts/dev-server.ps1
```

**Terminal B — UI**

```powershell
cd loom-studio
npm install
npm run dev
# http://127.0.0.1:5173
```

Open Studio: pilot fixture loads, chip **loom-server** green, **Validate** / **Run**
/ **Export** / Library / Forge.

## CLI-only path

```powershell
cd weavatrix-loom
cargo run -p wvx-cli -- validate fixtures/pilot-json-pipeline.wvx.json
cargo run -p wvx-cli -- run fixtures/pilot-json-pipeline.wvx.json
cargo run -p wvx-cli -- export-rust fixtures/pilot-json-pipeline.wvx.json -o $env:TEMP/loom-out --check
powershell -File ./scripts/alpha-smoke.ps1
```

## Transport rules (hard)

```text
Studio ──HTTP──► loom-server ──► wvx-command-bus ──► libs
CLI    ────────────────────────► wvx-command-bus ──► libs
Agent  ──MCP──► wvx-mcp (optional) ──► bus         # not product
```

See [crate-surface.md](crate-surface.md).

## Version

Workspace package version remains `0.1.0` with **alpha** maturity label in docs.
Promote to public beta after: Weavatrix facts feed path, second capability family,
and ratchet architecture baselines under real use.
