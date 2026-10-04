# dragons-frogma

A Dragon's Dogma 2 mod that shows friends' positions as ghost overlays
while each player runs their own save. 2-6 people, all on a shared
Tailscale/ZeroTier overlay. REFramework Lua + a small native Rust
plugin. Shared by `git clone`, installed by hand.

Not a product. Not a release. See `knowledge/adr/0001-charter.md`.

## Repo map

    crates/
      frogma-wire/      42-byte snapshot wire format (encode/decode, tests)
      frogma-peer/      UDP rx/tx threads, PeerTable ring buffer, staleness prune
      frogma-harness/   loopback multi-peer integration harness
      frogma-plugin/    cdylib → frogma_plugin.dll (loaded by REFramework)
    knowledge/          ADRs, constraints, invariants, questions, findings
      adr/              architectural decision records
      context/          constraints that actually apply
      graph/            seed.jsonl (manifest) + cids.lock (deterministic snapshot)
      schemas/          canon.d JSON schemas
    k-stack/            content-addressed DAG tool (MCP server, pinned commit)
    docs/               runbooks and operational guides
    .mcp.json           registers k-stack as a project MCP server
    CLAUDE.md           project-local Claude instructions (k-stack + graph workflows)

## Build & test

    make test        # cargo test --workspace (all frogma crates)
    make harness     # run the 3-peer loopback harness
    make seed        # regenerate knowledge/graph/cids.lock
    make k-stack     # build the k-stack CID tool
    make check       # cargo check --workspace

The Windows plugin DLL cross-compile path lives in
`docs/runbook-leg-a.md`.

## The two-layer structure

**Reasoning layer** lives in `knowledge/`. ADRs, constraints,
invariants, questions, and findings are tracked as a DAG in
`graph/seed.jsonl` with deterministic content hashes in
`graph/cids.lock`. Divergence between collaborators is resolved by
diffing the lock file and walking lineage. See `knowledge/README.md`.

**Runtime layer** is boring UDP. A native plugin opens a socket,
broadcasts 42-byte snapshots at 10 Hz, receives others' snapshots into
a per-peer ring buffer, and exposes them to Lua. Lua reads the DD2
camera + local player via REFramework's IL2CPP surface, projects peer
positions to screen space, and draws markers. No session layer, no
relay, no reliability. See `knowledge/adr/0002-transport.md`
and `knowledge/adr/0003-rendering.md`.

## Status

- **Q-0001 transport half:** done on Linux. 8/8 tests green.
- **Q-0004 (plugin loads in DD2):** in progress. Current leg.
- **Q-0002 (IL2CPP camera/player):** not started.
- **Q-0003 (render-hook thread safety):** not started.

See `knowledge/graph/seed.jsonl` for the full question/finding set.

## License

MIT. See `LICENSE`. This project ships no Capcom assets and performs
no game-state writes.
