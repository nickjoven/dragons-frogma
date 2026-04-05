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
      schemas/       canon.d JSON schemas (decision, component, invariant)
      adr/           architectural decision records (markdown, front-matter)
      graph/
        seed.jsonl   parent/child edges for initial ingest
      scripts/
        seed.sh      populates .ket/ via k-stack MCP tools

## Seeding the graph

Build k-stack once:

    (cd k-stack && cargo build --release)

Then populate:

    ./knowledge/scripts/seed.sh

This writes the content-addressed store to `.ket/` (gitignored — the store is
derivable from the tracked artifacts in `knowledge/`).

## Resolving divergence

When two branches both modify an ADR:

    # On branch A
    cid_a=$(ket put --kind=adr < knowledge/adr/0003-asynchrony.md)

    # On branch B (after merge attempt)
    cid_b=$(ket put --kind=adr < knowledge/adr/0003-asynchrony.md)

    # If cid_a == cid_b, the merge is a no-op. Otherwise:
    ket lineage $cid_a
    ket lineage $cid_b
    # walk back to the shared ancestor, decide which chain to keep.

The MCP tools (`ket_lineage`, `ket_align`, `ket_children`, `ket_topology`) do
the same from inside an LLM session.

## MVP charter (summary)

Transparent, asynchronous, event-driven, Lagrangian-relaxed multiplayer for
Dragon's Dogma 2. Steam client, no rights reserved. The four adjectives are
load-bearing — each has its own ADR and each downstream component cites them
as parents. See `adr/0001-mvp-charter.md`.
