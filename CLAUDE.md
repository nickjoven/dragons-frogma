# dragons-frogma — project-local instructions

DD2 ghost-overlay mod. Pet-project scope (see `knowledge/adr/0001-charter.md`).
Global engineering protocol lives in `~/.claude/CLAUDE.md` and applies here.
This file adds project-specific tooling guidance, mostly around the **k-stack**
MCP server and the knowledge graph it backs.

---

## k-stack MCP server

k-stack is registered in `.mcp.json` at the repo root and auto-starts when you
run Claude Code from this directory. It exposes **15 tools** over stdio JSON-RPC
against the content-addressed store at `.ket/` (gitignored, per-project).

Verify with `/mcp` — you should see `k-stack` with 15 tools listed.

### Tool inventory

**CAS — raw content hashing.** Use when you want content addressability without
DAG semantics.
- `ket_put(content, kind)` → `cid` — store bytes, return BLAKE3 CID.
- `ket_get(cid)` → `content, size` — retrieve by CID.
- `ket_verify(cid)` → `valid` — re-hash and confirm integrity.

**DAG — reasoning chains with parents.** Use when producing an artifact that
derives from existing nodes.
- `ket_store(content, kind, parents[], agent)` → `node_cid, content_cid` — store
  content and create a DAG node linking to parents. Records agent + kind +
  timestamp in the node header.
- `ket_lineage(cid, max_depth?)` → `chain[]` — walk parents backward to the root.
- `ket_children(cid)` → `children[]` — forward reachability from a node.

**Schema (canon.d) — structural validation.**
- `ket_schema_store(name, version, fields[])` → `cid`
- `ket_schema_list` → `schemas[]`
- `ket_schema_validate(schema_cid, content)` → `valid, errors?`
- `ket_canonicalize(schema_cid, content)` → `cid, canonical_bytes_hex` —
  deterministic encoding. Same content + schema = same CID always.
- `ket_schema_stats(schema_cid)` → `total_nodes, unique_outputs, dedup_ratio`

**Structure — schema drift analysis.**
- `ket_align(source_schema_cid, target_schema_cid, min_confidence?)` →
  `candidates[]` with confidence + rationale. Field-mapping between schemas.
- `ket_topology(kind?)` → `clusters[], convergent_clusters, co_occurrences[]`.

**Query.**
- `ket_search(query)` → `matches[]` — full-text search across stored content.
- `ket_recent(limit?, kind?)` → `nodes[]` — sorted by timestamp.

---

## When to use k-stack tools vs. editing files

This project has **two layers** that look similar but serve different purposes:

1. **`knowledge/graph/seed.jsonl`** — the *source of truth* manifest. Human-
   edited JSONL. Each line declares a node (ADR, constraint, invariant,
   question, finding) with its id and parents. Edit this file directly when
   adding new nodes; never use `ket_store` to bypass it.

2. **`.ket/` CAS store** — the *materialized* content-addressed store. Produced
   by `knowledge/scripts/seed.py` running `ket_store` calls over `seed.jsonl`.
   Regenerate with `make seed`. Do not edit by hand.

**Rule:** mutations to the knowledge graph go through `seed.jsonl` → `make seed`.
Use `ket_*` tools for **queries** (lineage, children, search, topology, align),
not for writes that should be in the manifest.

**Exception:** ephemeral reasoning artifacts that don't belong in the permanent
record (session notes, scratch derivations) may be stored directly via
`ket_store`. They land in `.ket/` but not in `seed.jsonl` or `cids.lock`.

---

## Node taxonomy (project conventions)

Every node in `seed.jsonl` has `kind`, `id`, `parents[]`, and kind-specific
fields. Schemas are in `knowledge/schemas/*.json`.

| kind | id prefix | example | parents typically point to |
|------|-----------|---------|----------------------------|
| `schema` | `schemas/*.json` | `schemas/decision.v1.json` | (none) |
| `decision` | `ADR-NNNN` | `ADR-0002` | prior ADRs, CTX |
| `context` | `CTX-NNNN` | `CTX-0001` | ADRs |
| `constraint` | `client.*` / `ops.*` | `client.patch-breaks-offsets` | `CTX-NNNN`, ADRs |
| `invariant` | `inv.*` | `inv.no-game-state-writes` | ADRs |
| `question` | `Q-NNNN` | `Q-0004` | ADRs, constraints, prior Qs |
| `finding` | `f.*` | `f.dd2-il2cpp-entry-points` | Qs, constraints, ADRs |

**Rules:**
- `constraint.ckind` is `"hard"` or `"soft"`.
- `question.status` is `"open"`, `"partial"`, `"answered"`, or `"stale"`.
- `finding.confidence` is `"low"`, `"medium"`, `"high"`, or `"verified"`.
- `finding` nodes must include `sources[]` (URLs or `git log` refs) and
  `accessed_at` (ISO date).
- `parents[]` are symbolic ids, resolved at seed time into the DAG.

---

## Common workflows

**Adding a new finding after research:**
1. Append a JSONL line to `knowledge/graph/seed.jsonl` with kind `finding`,
   parents pointing at the question it answers + any constraints it touches.
2. Run `make seed` to regenerate `cids.lock`.
3. If the finding answers a question, also update that question's `status`
   in the same edit.

**Adding a new open question:**
1. Append JSONL line with kind `question`, status `"open"`, parents pointing
   at the ADR or constraint that motivates it.
2. Run `make seed`.

**Resolving divergence with a collaborator:**
1. Diff `cids.lock` line-by-line — lines with differing `content_cid` are
   the diverged artifacts.
2. Use `ket_lineage` on the diverged CIDs to walk back to the shared ancestor.
3. Human decides; update `seed.jsonl`; re-run `make seed`.

**Checking what depends on a node before editing it:**
- `ket_children(cid)` — everything downstream will need review if the node
  changes meaning (not just wording).

---

## Schema validation

Before adding a new finding/question/etc., check the schema:
- `knowledge/schemas/finding.v1.json`, `question.v1.json`, etc.
- Schemas are intentionally loose (fields like `parents[]` aren't in the
  schema but appear in `seed.jsonl` entries) — prefer mirroring existing
  entry shape over strict schema conformance.

---

## Things not to do with ket tools

- **Don't `ket_put` Capcom assets, decompiled DD2 code, or save-file contents.**
  `client.no-asset-redistribution`, `client.save-file-is-sacred`. The CAS has
  a secret-pattern filter but not an asset filter — discipline is yours.
- **Don't duplicate `seed.jsonl` content into the CAS via `ket_store`.**
  `make seed` already does that deterministically. Duplicates waste storage
  (same content = same CID = no duplicate, but double-writing is still noise).
- **Don't cache CIDs across sessions without re-verifying** — `.ket/` is
  gitignored and per-checkout. A CID from your previous session may not exist
  in a fresh clone until `make seed` runs.

---

## Related docs

- `knowledge/README.md` — k-stack rationale + divergence-resolution workflow
- `knowledge/adr/0001-charter.md` — project scope and non-goals
- `knowledge/context/constraints.md` — the full constraint table
- `k-stack/README.md` — upstream k-stack tool docs
- `Makefile` — `seed`, `test`, `check`, `harness`, `k-stack` targets
