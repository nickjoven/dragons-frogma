# ADR-0003: Asynchrony via store-and-forward relays

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001, ADR-0002

## Context

Players play offline, on planes, with flaky wifi. Lockstep or tick-synchronized
multiplayer is a non-starter. We need a transport that treats the network as
best-effort and tolerates arbitrary delay between publish and observe.

## Decision

Events flow through **store-and-forward relays**. A relay accepts event CIDs +
blobs, fans them out to interested subscribers, and retains them for a TTL.
Clients poll or subscribe when online. Ordering is *causal*, not *total*:
events carry a `causes[]` list of parent event CIDs, and consumers apply them
in a topological order consistent with cause.

No global clock. No tick. No authoritative server state.

## Consequences

- A player returning after 3 days sees 3 days of events in causal order.
- Two players in the same world may apply the same events in different wall-
  clock orders, as long as causal order is preserved.
- Relay is stateless w.r.t. game logic — it's a dumb pipe over CIDs.

## Invariants seeded

- `inv.causal-before-total`
- `inv.no-global-clock`
