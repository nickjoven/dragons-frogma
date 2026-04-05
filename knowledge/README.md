# knowledge/

Canonical architectural + schematic record for the dragons-frogma multiplayer
mod MVP. Backed by [k-stack](../k-stack/) (content-addressed DAG, exposed as
MCP tools).

## Why this exists

Design histories diverge. Two agents, two branches, two sessions — each writes
a plausible-looking ADR and neither references the other. Later: which one was
actually decided? Which assumption is load-bearing? Git tracks the files but
not the *reasoning chain* between them.

k-stack gives every artifact a CID (BLAKE3 of canonical bytes) and records its
parents. Divergence is resolved the same way git resolves it for code:

1. **Same CID** → identical artifact, no conflict.
2. **Different CID, shared parent** → genuine fork. Use `ket_lineage` to walk
   both sides, `ket_align` for schema drift, human decides.
3. **No shared parent** → one side is stale. `ket_children` on the ancestor
   shows which branch the graph has actually grown.

## Layout

    knowledge/
      schemas/       canon.d JSON schemas (decision, invariant, question, finding)
      adr/           architectural decision records (markdown)
      context/       constraints that actually apply
      graph/
        seed.jsonl   parent/child edges, the authoritative manifest
        cids.lock    deterministic content_cid snapshot (diff oracle)
        nodes.run    ephemeral per-run node_cids (gitignored)
      scripts/
        seed.py      drives k-stack over stdio JSON-RPC, regenerates lock

## Seeding the graph

Build k-stack once:

    (cd k-stack && cargo build --release)

Then populate:

    KET_HOME=.ket python3 knowledge/scripts/seed.py

This writes the content-addressed store to `.ket/` (gitignored) and rewrites
`knowledge/graph/cids.lock`. The lock pins **content_cid** (deterministic,
re-running on unchanged inputs produces an identical file). node_cids live in
`nodes.run` and are ephemeral because `ket_store` embeds a timestamp in the
DAG node header — diff `cids.lock` for divergence detection, ignore node_cid
differences.

## Resolving divergence

Two contributors each run `seed.py`. Diff their `cids.lock` files. Any line
with a differing `content_cid` points at the artifact that diverged. Use
`ket_lineage` (MCP) to walk the parents back to the shared ancestor.

## Charter (summary)

Pet-project presence mod for Dragon's Dogma 2: render friends' positions as
ghost overlays while each player plays their own save. REFramework Lua +
small native UDP plugin, friends on a Tailscale overlay, no distribution.
See `adr/0001-charter.md`.

**k-stack is for documentation; the runtime is boring UDP.**
