# Domain roadmap (Gate C bottleneck)

**Date:** 2026-08-13  
**Status:** architecture freeze for new fundamentals — grow **semantic domains**

## Priority shift

| Was | Now |
|-----|-----|
| New IR / abstractions | **Frozen** unless Gate C forces a real gap |
| Single JSON vertical | **Multi-domain proof** |

The largest remaining product problem is **not** duplication or packaging.

It is: Loom is still proven mainly on **one semantic family** (JSON transforms).
Gate C documents that honestly. Full Gate C remains open until:

1. **≥5 external conformant implementations** (across domains)
2. **Multi-domain sample** projects
3. **Human review metrics**
4. **Real Forge economics** (not only the pilot harness)

That is the **#1 project bottleneck**.

## Domain sequence

### Domain 1 — JSON transforms (done)

```text
bytes → json_value → json_value → bytes
```

Multi-impl parse/serialize/path_set. Pilot fixture: `pilot-json-pipeline.wvx.json`.

### Domain 2 — Hashing (in progress / pilot)

```text
bytes → digest
```

| Capability | Contract | Implementations (pilot) |
|------------|----------|-------------------------|
| `data.hash.sha256@1` | bytes → digest (32 B) | **4 multi-impl:** one-shot · streaming · chunked · update-all (`sha2.*`) |
| `data.hash.blake3@1` | bytes → digest (32 B) | `blake3.blake3@1` |

**Why hashing first**

- Tiny semantic contract
- Huge implementation diversity (pure Rust, SIMD, FFI, crypto-provider, target-specific)
- Axes to score: correctness, output equality, performance, target support, unsafe/FFI, license, security

Fixture: `fixtures/pilot-hash-pipeline.wvx.json`.

### Domain 3 — Compression (pilot)

```text
bytes → gzip-bytes → bytes   (round-trip)
```

| Capability | Multi-impl (3 each) |
|------------|---------------------|
| `data.compress.gzip@1` | flate2 write_all · chunked · oneshot-read |
| `data.compress.gunzip@1` | flate2 read_to_end · chunked · take |

Fixture: `fixtures/pilot-compress-pipeline.wvx.json`.  
Compressed **byte equality** is not required across gzip impls; **gunzip equality** is.

### Domain 4 — Binary codecs

```text
bytes ↔ encoded bytes / structured wire
```

Examples: base64, hex, protobuf-like encode/decode. Natural place for **FerroSift**
ops as Loom Implementations (not importing FerroSift into Loom core) — ADR-0012.

## What not to do now

- New graph IR concepts “because interesting”
- Dual code graph inside Loom
- Product MCP for Studio
- Splitting monorepo before API freeze
- Treating Forge bootstrap AST as the product story (Weavatrix facts preferred)

## Gate C checklist

- [x] 5+ external packages in `fixtures/gate-c-external` (JSON×2 + hash + gzip + gunzip)
- [x] Multi-domain sample (JSON + hash + compress)
- [x] Human minutes field (`--human-minutes`, measured override)
- [x] Forge economics on external package tree (`forge gate-c --external … --check`)
- [ ] Production marketplace / public Registry Go (still out of scope)

```bash
cargo run -p wvx-cli -- forge gate-c --external fixtures/gate-c-external --human-minutes 45 --check
# go=true · extract=1.00 · map=1.00 · compile=1.00 · human_min=45
```

See also: [go-no-go-c-pilot.md](go-no-go-c-pilot.md), [beta-prototype.md](beta-prototype.md).
