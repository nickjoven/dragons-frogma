# BFS sweep of the frontier (2026-04-05)

Eight unknowns visited, all breadth, no depth. The goal is a map of
what-exists, not conclusions. Each branch becomes its own question node with
provenance; go back and deepen later.

## Branches visited

| id | question | status after sweep |
|----|----------|--------------------|
| Q-0003 | DD2 patch cadence 2024→2026? | answered (high) |
| Q-0004 | Capcom's posture toward RE-Engine mods? | answered — **hostile** (high) |
| Q-0005 | DD2 active player / mod velocity? | partial (medium) |
| Q-0006 | Steam GNS license for third-party use? | answered (verified) |
| Q-0007 | DD2 pawn protocol — publicly RE'd? | answered — **no** (high) |
| Q-0008 | REFramework Lua→native DLL ABI? | partial (medium) |
| Q-0009 | DD2 online/offline boundary? | answered (high) |
| Q-0010 | DD Online private server revival? | discovered — worth deep dive |

## What this map changed

Three things the BFS actually shifted, worth recording:

### 1. `dist.dmca-surface` was under-specified

Captured before as "player-authored content on relays has DMCA exposure." The
sweep shows Capcom's DMCA posture is broader than that:

- **2017**: DMCA'd SF5 costume modders (paywalled mods, some used third-party
  IP like 2B from Nier).
- **2023**: A wider takedown campaign against **YouTube videos containing
  any RE-Engine mods** — across DMC, RE, MHR. Not just paywalled.

Implication: the exposure is not just to *hosting infringing content*, it's
to *visibility of the mod itself*. A networking mod is structurally more
visible than a cosmetic one. Supersedes `dist.dmca-surface` with
`dist.capcom-hostile-dmca-posture` (hard) + `dist.avoid-video-promotion`
(soft, strategic).

### 2. DD2 has a **native** pawn-sharing network — and it's the entire prior art

DD2 already has a Capcom-hosted cross-player pawn sharing system. When
offline, the game explicitly denies pawn rental. Players already accept
"some cross-player features require being online" as a base-game contract.

We can ride that social contract without matching the mechanism.

But: **no public reverse-engineering of DD2's pawn protocol exists.** And
Capcom's C&D history makes publishing one risky. Implies: don't RE
Capcom's protocol; build our own event bus in parallel.

### 3. Dragon's Dogma Online private server revival is the closest prior art

DD Online (2015–2019, JP-only MMO, servers shut down) has community
revival projects — private server emulators. This is *closer* to what we
want than Elden Ring Seamless Co-op in one crucial dimension: **they
reverse-engineered a Capcom game's netcode and lived**. Worth studying
what posture Capcom took toward them (Q-0010 deep dive).

## Also-confirmed

- **Steam GNS is BSD-3-Clause**, no Steam required. Transport primitive is
  essentially free, permissively licensed, Valve-maintained.
- **DD2 patch cadence is slowing**: 2.01 (Sep 2024) → Jan 2025 → Jan 2026
  → quiet since. `client.patch-breaks-offsets` clock ticks less often than
  we feared. Good for mod viability; bad if "slowing" means "abandoned" (it
  doesn't seem to — UDD2P activity suggests sustained Capcom engagement).
- **REFramework Lua→native DLL** works but has an environment quirk
  (GitHub issue #623): vanilla `require` semantics don't always apply.
  Need to use `reframework_plugin_initialize` plugin ABI, not plain Lua
  `loadlib`.

## Sources

- Capcom DMCA history: https://www.pcgamer.com/street-fighter-costume-modders-issued-dmca-takedown-notice-from-capcom/ ,
  https://powerupgaming.co.uk/2023/12/11/capcom-is-striking-down-videos-with-mods-in-their-games/ ,
  https://www.resetera.com/threads/capcom-is-issuing-copyright-strikes-takedown-requests-on-monster-hunter-videos-featuring-mods.793434/
- DD2 patch notes: https://steamdb.info/app/2054970/patchnotes/ ,
  https://www.dragonsdogma.com/2/en-us/topics/update/
- Steam GNS: https://github.com/ValveSoftware/GameNetworkingSockets ,
  https://news.ycombinator.com/item?id=16689523
- REFramework plugin loading: https://github.com/praydog/REFramework/issues/623 ,
  https://cursey.github.io/reframework-book/
- DD2 online/offline: https://beebom.com/dragons-dogma-2-offline-mode/ ,
  https://www.dualshockers.com/dragons-dogma-2-how-to-play-offline/
- DD1 MP mod attempts: https://steamcommunity.com/app/367500/discussions/0/451850849183083339/
- DD Online private server reference: https://opsc.dark-arisen.com/

## What comes next

These questions remain **open** and warrant depth later, not now:

- Q-0004 deep dive: has Capcom ever DMCA'd a **networking** mod specifically,
  vs. cosmetic/video mods?
- Q-0007 deep dive: can we avoid RE'ing Capcom's protocol entirely by using
  Lua/IL2CPP surface reads (positions, HP, flags) without touching Capcom's
  network stack at all?
- Q-0010 deep dive: what did Capcom's legal team do (or not do) about DD
  Online private servers?
- Q-0011 (new): what's the smallest REFramework plugin that can run a GNS
  client loop off-thread without blocking the render pipeline?
