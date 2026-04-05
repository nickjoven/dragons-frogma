# ADR-0005: Lagrangian relaxation of global invariants

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001, ADR-0004

## Context

A transparent async system cannot enforce hard global invariants like "all
players see the same dragon HP at time T" — the network makes it impossible,
and trying would require a central authority, which the charter forbids.

We still care about those invariants; we just can't treat them as hard
constraints. We need a way to *prefer* convergence without *requiring* it.

## Decision

Model each global invariant `g_i(state) = 0` as a **soft constraint** with a
Lagrangian penalty `λ_i · ||g_i(state)||`. A projection's reducer minimizes

    L(state) = objective(state) + Σ λ_i · penalty_i(state)

where `objective` is the local player's view utility and `λ_i` is the
per-invariant weight. Concretely:

- **Dragon HP disagreement** → small penalty; clients nudge toward the minimum
  observed HP when they merge event streams.
- **Pawn position disagreement** → larger penalty; clients snap late.
- **Quest completion conflict** → very large penalty; clients prefer the
  earliest causally-consistent completion event.

λ values are configuration, not consensus. Players can tune them. The system
measures divergence (via `ket_align` + DAG distance) and reports it — it does
not eliminate it.

## Consequences

- No rollback, no rewind, no "server says no." Just soft pull toward agreement.
- Divergence becomes a first-class, measurable metric rather than a bug.
- Tuning λ is an ongoing empirical concern, not a one-time decision.
- Projections need not be monotone, but their penalty functions must be
  bounded so the Lagrangian is stable.

## Invariants seeded

- `inv.soft-global-constraints`
- `inv.divergence-is-measured`
- `inv.no-rollback`
