# Loom registry-dev

Local development catalog of **capabilities** and **implementations** for
Weavatrix Loom `0.1`.

This tree is the working stand-in for the future public `weavatrix-registry`
repository. Layout:

```text
registry-dev/
├── capabilities/       # capability contracts (JSON)
├── implementations/    # implementation manifests (JSON)
├── index/              # flat indexes for quick listing
└── README.md
```

## Status

All entries are **pilot** quality: enough for the JSON pipeline vertical slice
and playground/export adapters. They are not a claim of production admission.

## Query

```bash
cargo run -p wvx-cli -- registry list
cargo run -p wvx-cli -- registry search json
cargo run -p wvx-cli -- registry implementations --capability data.json.parse@1
```

Default path is `./registry-dev` relative to the process cwd (or set
`WVX_REGISTRY`).
