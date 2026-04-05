# ADR-0006: Pet-project scope — zero distribution as the floor

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001
- **Partially supersedes:** (see "What this moots" below)

## Context

The charter (ADR-0001) framed the mod as "Steam-distributed, no rights
reserved" — language that implied a public distribution path (Fluffy,
Nexus, GitHub releases) and the operational weight that comes with it:
packaging, compat testing, DMCA posture management, relay operations,
documentation for non-technical users.

The BFS sweep (Q-0003..Q-0011) surfaced the true cost of that path:
Capcom's RE-Engine DMCA posture is hostile, promotion-via-video is a
trigger, and the solo-maintainer pattern (`pat.single-maintainer-seed`)
only survives when distribution is tiny and quiet.

The maintainer is now explicit: **this is a pet project. Zero public
distribution is an acceptable floor.**

## Decision

The working scope floor is: **me + a handful of friends who already run
DD2 + REFramework**, sharing the mod via `git clone` and manual install.
No Nexus, no Fluffy bundle, no release artifacts, no telemetry, no
promotion. Presence-only (ghosts, per `pat.ghost-before-state`). If the
thing ever gets good enough that someone asks for a release, *that's a
later decision, made deliberately, not by drift*.

## What this moots

The following nodes no longer pressure the architecture under current
scope. They stay in the graph (CAS is append-only), but are now dominated
by ADR-0006 for design purposes:

- `dist.steam-workshop-gated` — irrelevant, no store
- `dist.avoid-video-promotion` — vacuous, no promotion
- `f.pak-fluffy-distribution` — irrelevant, git-clone install
- `dist.eu-data-residency` — friends self-host or trust the maintainer's
  one box; no GDPR surface beyond a personal-contacts list
- `net.relay-cheap-but-not-free` — 10k CIDs/day budget is ~100× actual need
- `dist.dmca-surface` (and `dist.capcom-hostile-dmca-posture`) — still
  informs behavior (don't taunt Capcom), but mooted operationally: a
  private mod shared among named friends is below Capcom's enforcement
  threshold per all observed precedent.

## What this strengthens

- `ops.solo-maintainer` — now load-bearing. No "what if we scale" escape.
- `ops.no-paid-infra` — a laptop or a home PC is the relay.
- `ops.telemetry-opt-in-only` — trivially satisfied: there is no telemetry.
- `pat.single-maintainer-seed` — aligns perfectly.
- `pat.ghost-before-state` — strongly enforced; state sync is out of
  scope entirely until ADR-0006 is itself superseded.
- `client.save-file-is-sacred` — unchanged and still hard.

## What this unlocks

- **Transport can be dumber.** Direct LAN / ZeroTier / a Tailscale tailnet
  among friends removes the NAT-traversal problem (`net.p2p-nat-hostile`)
  without needing Steam GNS at all. GNS remains the fallback if friends
  aren't on a shared overlay.
- **Iteration speed.** Breaking changes are a group-chat message, not a
  migration. Schema drift can be resolved by `ket_align` among a known
  small set of peers.
- **No packaging work.** The mod *is* the git repo. Contributors clone it.
- **Low-key REFramework plugin shape.** Q-0011's "smallest REFramework+GNS
  plugin" becomes "smallest REFramework+UDP plugin" — even simpler.

## Implication for the pillars (ADR-0001 through ADR-0005)

All four pillars still apply, but their motivation shifts from "survive in
the wild" to "keep the design honest":

- **Transparent** (ADR-0002): still CID-based, because it makes the system
  debuggable for me, not because auditability matters to strangers.
- **Asynchronous** (ADR-0003): still store-and-forward, because friends
  play at different times.
- **Event-driven** (ADR-0004): still projections, because it's the only
  way to keep a multiplayer layer from corrupting save files.
- **Lagrangian-relaxed** (ADR-0005): still soft constraints, because
  even among friends the network lies.

## Exit criteria (when to supersede)

Supersede ADR-0006 only when:
1. Someone outside the friend group asks for a copy, **and**
2. I want to say yes, **and**
3. I'm willing to accept the distribution tax documented above.

None of these are forecasted. They are not goals.
