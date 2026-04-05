# ADR-0002: Transparency via content-addressed events

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001

## Context

"Transparent" in the charter means two things: (1) a player can always audit
what events they published and received, and (2) two players comparing notes
can prove whether they saw the same event or not.

## Decision

Every cross-player event is hashed (BLAKE3) and addressed by its CID. Events
are stored locally in a k-stack CAS before being relayed. Relays never mutate
event bodies — they only move CIDs + blobs.

A player's outbox and inbox are append-only DAGs. Disputes are resolved by
exchanging CIDs: if both players have the same CID, they saw the same event,
byte-for-byte. If not, the content itself is the proof of divergence.

## Consequences

- Cheating by forging someone else's events is impossible without CID collision.
- Replay/audit is free: walk the DAG.
- Storage grows monotonically; need a retention policy (future ADR).

## Invariants seeded

- `inv.cid-equals-proof`
- `inv.append-only-inbox`
