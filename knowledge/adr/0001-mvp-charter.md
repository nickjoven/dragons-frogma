# ADR-0001: MVP Charter — dragons-frogma

- **Status:** accepted
- **Created:** 2026-04-05
- **Supersedes:** —

## Context

Dragon's Dogma 2 ships as a single-player game. Players want shared presence:
seeing each other's pawns, world events, and achievements without Capcom-hosted
infrastructure and without modifying the retail client's memory in ways that
trip anti-cheat or break offline play.

The mod must be safe to install (no rights reserved over game assets), cheap to
operate, and tolerant of players who play offline for days at a time.

## Decision

Ship a **transparent, asynchronous, event-driven, Lagrangian-relaxed** multiplayer
mod as a Steam-distributed client:

- **Transparent:** every cross-player state change is observable and auditable
  by its originating player. No hidden server-authoritative simulation.
- **Asynchronous:** no lockstep, no tick synchronization. Peers publish events
  when they happen; consumers apply them when they're received.
- **Event-driven:** all cross-player state is expressed as discrete, causally-
  linked events. No shared mutable world state.
- **Lagrangian-relaxed:** hard global invariants (e.g. "every player sees the
  same dragon HP") are relaxed into soft constraints with penalty terms.
  Divergence is tolerated, measured, and nudged toward convergence rather than
  forced.

## Consequences

- No central authority → cheap to run, resilient to Capcom patches.
- Players can desync temporarily; the system must make desync *legible* and
  *recoverable*, not prevent it.
- All architectural decisions below inherit from this charter. Changes to any
  of the four pillars supersede this ADR.

## Invariants seeded

- `inv.no-rights-reserved`
- `inv.offline-first`
- `inv.player-owns-events`
