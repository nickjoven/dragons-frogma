# ADR-0001: Charter — pet-project presence mod for DD2

- **Status:** accepted
- **Created:** 2026-04-05

## What this is

A Dragon's Dogma 2 mod that shows my friends' positions as ghosts in my
world while we each play our own save. Between 2 and ~6 people. Built on
REFramework. Shared by `git clone`, installed by hand.

Not a product. Not a release. Not a distribution effort.

## What we build

- A REFramework Lua script + a small native plugin DLL.
- The plugin opens a UDP socket on a friends-only overlay network (assume
  Tailscale / ZeroTier / LAN — not our problem to solve).
- Every N ms, we broadcast our own avatar state: position, facing, HP,
  maybe vocation / pose.
- We receive others' state, interpolate it, and draw ghost overlays via
  REFramework's render hook.

That's v0.1. That's the whole MVP.

## What we don't build

- No writes to DD2 game state. No pawn control, no quest flag sync, no
  inventory sharing, no save-file touching.
- No distribution artifacts: no Nexus page, no Fluffy bundle, no release
  tarball, no YouTube, no blog.
- No content-addressed event bus, no DAG of runtime events, no Lagrangian
  reconciliation, no append-only inboxes. **The knowledge graph (k-stack)
  is for documentation; the runtime is boring UDP.**
- No Capcom-netcode reverse engineering. Ever. Read game state through
  REFramework's IL2CPP surface only.
- No NAT traversal code. Friends are on a shared overlay or it doesn't
  work. That is acceptable.

## Why these lines

This is a pet project. The maintainer is one person. Every line of scope
added is paid for in weekends. The things above were considered and
rejected because they're expensive, don't earn their keep at this scale,
or (in the case of netcode RE) invite specific known risks.

## Exit criteria for expanding scope

Supersede this ADR only when:
1. Someone outside the friend group asks for a copy, **and**
2. I want to say yes, **and**
3. I'm willing to accept the packaging, support, and DMCA posture cost.

None of these are goals.
