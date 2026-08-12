# Pilot admission policy (automated check)

**Scope:** consistency of declared lifecycle labels vs multi-fact evidence.  
**Not:** full Gate E (reproducible benchmarks, provenance, human admission board).

See ADR-0007 (public evidence / private lab) and ADR-0008 (no readiness %).

## Commands

```bash
cargo run -p wvx-cli -- registry check
# aliases: registry audit | registry admission
# HTTP: GET /api/v1/registry/admission
```

Exit code **1** if any **overclaim** is found.

## Ranking

```text
inventory_only < candidate < conformant < admitted
```

## Justified status (max allowed by evidence)

| Label | Requirements (pilot v0.1) |
| --- | --- |
| `inventory_only` | Default when nothing else applies |
| `candidate` | Adapter present **or** `build=pass`; no path to higher if axes fail |
| `conformant` | Adapter + `conformance=pass` + `build≠fail` + no axis `fail` |
| `admitted` | Adapter + `build`/`conformance`/`license`/`security`/`benchmark` all **pass** |

Any axis `fail` caps the justified label at `candidate` (or `inventory_only` without adapter).

## Check outcomes

| Case | Severity | CLI |
| --- | --- | --- |
| Declared **>** justified | **error** (overclaim) | fail |
| Declared **<** justified | warning (underclaim) | pass |
| Declared `admitted` | info + overclaim if not justified | fail if overclaim |

## Pilot registry-dev

Transform impls are labeled `conformant` with `conformance: pass` (Gate A vectors).  
I/O is `candidate`. Nothing is `admitted`. `registry check` should **PASS**.

## What this does *not* do

- Does not run conformance or benchmarks itself
- Does not write manifests or auto-promote labels
- Does not replace human policy for production admission
