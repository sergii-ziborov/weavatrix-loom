# Evidence sidecars (Gate E pilot)

Files here are **multi-fact evidence** records, not a readiness percentage.

| Pattern | Meaning |
|---------|---------|
| `*.provenance.json` | Written by `wvx registry admit` (host + human review + bench fingerprint) |

Generate a bench report (usually under repo `.lab/`, gitignored):

```bash
cargo run -p wvx-cli -- bench -o .lab/bench.json
```

See [`docs/go-no-go-e-pilot.md`](../../docs/go-no-go-e-pilot.md).
