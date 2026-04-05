# Q-0001: How do single-player games wind up with community-borne multiplayer mods?

- **Status:** partial
- **Opened:** 2026-04-05
- **Why it matters:** the charter claims a lane that other mods have traveled.
  Understanding their trajectories — what worked, what killed them — tells us
  what to imitate and what to refuse.

## The pattern, distilled from precedent

Community SP→MP mods that *ship* and *persist* share a recognizable shape.
They do not appear all at once. They accrete in a specific order, on top of
a specific substrate, under specific pressures.

### Seven observed regularities

| id | pattern |
|----|---------|
| `pat.hook-framework-first` | A mature single-player mod loader exists **before** anyone attempts MP. The MP mod is a client of that loader, not a rewrite of it. |
| `pat.single-maintainer-seed` | 1–2 obsessive maintainers drive the first 2–3 years. Team projects of 5+ die during the first major host-game patch. |
| `pat.ghost-before-state` | v0.1 shows other players' avatars/positions. State sync (inventories, quest flags) comes years later, if ever. |
| `pat.p2p-via-platform-sockets` | Successful modern mods use Steam GNS / Epic EOS for NAT traversal. Running dedicated infra is where volunteer mods burn out. |
| `pat.desync-is-feature-not-bug` | Persistent mods treat local divergence as expected. Cosmetics differ, monsters spawn differently, quest flags fork — and nobody cares. |
| `pat.publisher-tolerance-is-load-bearing` | The mod lives or dies by the publisher's posture. FromSoft + Bethesda have been tolerant; Capcom's posture is *title-dependent*. |
| `pat.version-brittleness-kills` | The single biggest cause of mod death is a host-game patch the maintainer can't keep up with. Patch cadence is the clock. |

## Reference cases

- **Elden Ring Seamless Co-op** (LukeYui, May 2022 → current). P2P over Steam
  GNS, 6-player parties, removes fog walls. Single maintainer. FromSoft
  tolerated. Closest structural precedent for what we want.
- **Skyrim Together Reborn** (2022 rewrite of Skyrim Together). Built on
  SKSE. Tolerated by Bethesda. State sync is partial and opt-in.
- **Nucleus Co-op** (different pattern — local split-screen virtualization).
  Worth noting only as a foil: it's not what we're doing.
- **JC2-MP / JC3-MP** (Just Cause series, Nanos team, 2011+). Dedicated
  servers, bespoke Lua scripting layer. Persisted ~a decade.
- **FiveM / RageMP** (GTA V). Rockstar ambivalence, eventual acquisition of
  FiveM. Relevant only as a cautionary tale about publisher capture.
- **MHW:World MP extensions** (Capcom title, 2018). Anti-cheat enforcement
  tightened over time. Capcom precedent we must watch.

## What this implies for dragons-frogma

- We sit downstream of **REFramework** (ADR-0001-adjacent: we don't build it).
- We should ship **presence first, state later** — not tempt ourselves into
  projection sync in v0.1.
- We should use **Steam GNS** for transport when possible, relay-only fallback
  (already in ADR-0003). This is not novel; it's the observed survival path.
- **Lagrangian relaxation (ADR-0005)** is not theoretical elegance — it's
  what every persistent SP→MP mod rediscovers empirically.
- **Patch cadence** is now an operational KPI. Every DD2 patch starts a clock
  on `client.patch-breaks-offsets`. (See Q-0003 when opened.)

## Gaps (open sub-questions)

- What is Capcom's specific posture toward DD2 networking mods? (Q-0004)
- What is DD2's actual patch cadence 2024-01 → 2026-04? (Q-0005)
- What volume of players remains active enough to sustain a relay? (Q-0006)
