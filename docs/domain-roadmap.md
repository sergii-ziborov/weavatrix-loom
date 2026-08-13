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
| `data.hash.sha256@1` | bytes → digest (32 B) | `sha2.sha256@1` one-shot · `sha2.sha256-streaming@1` (equal digests, different path) |
| `data.hash.blake3@1` | bytes → digest (32 B) | `blake3.blake3@1` |

**Why hashing first**

- Tiny semantic contract
- Huge implementation diversity (pure Rust, SIMD, FFI, crypto-provider, target-specific)
- Axes to score: correctness, output equality, performance, target support, unsafe/FFI, license, security

Fixture: `fixtures/pilot-hash-pipeline.wvx.json`.

### Domain 3 — Compression (next)

```text
bytes → compressed-bytes
```

Example: gzip. Forces: compression level, streaming, deterministic output, dictionary,
memory/CPU tradeoffs. If Loom survives compression **without semantic collapse**,
that is strong evidence.

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

## Gate C checklist (open)

- [ ] 5+ external conformant implementations (count across domains)
- [ ] Multi-domain sample suite (JSON + hash + …)
- [ ] Human review metrics sheet
- [ ] Forge economics on real packages (not only monorepo fixture)

See also: [go-no-go-c-pilot.md](go-no-go-c-pilot.md), [beta-prototype.md](beta-prototype.md).
