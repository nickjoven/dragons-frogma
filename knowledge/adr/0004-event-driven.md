# ADR-0004: Event-driven state via derived projections

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001, ADR-0003

## Context

The charter says "no shared mutable world state." But players still need to
*see* a coherent world: pawn locations, dragon HP, quest progress. Something
must derive viewable state from the raw event stream.

## Decision

Cross-player state is expressed exclusively as events. Views are derived
**projections** over the causal event DAG, computed locally per-client. Each
projection declares:

- the event `kind`s it consumes
- a reducer `(state, event) -> state`
- a relaxation policy (see ADR-0005) for handling missing/late events

Projections are pure functions of the DAG. Two clients with the same DAG
prefix compute the same projection — and disagreements reduce to DAG diffs.

## Consequences

- No "server state" to desync against; there's only the event DAG.
- New projections can be added without schema migration of stored events.
- Memory cost scales with projection count × DAG size; projections must be
  incremental.

## Invariants seeded

- `inv.projections-are-pure`
- `inv.state-is-derived`
