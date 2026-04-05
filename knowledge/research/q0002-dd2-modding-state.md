# Q-0002: What does Dragon's Dogma 2 support now (April 2026)?

- **Status:** partial
- **Opened:** 2026-04-05
- **Why it matters:** the mod's surface area is whatever REFramework exposes
  plus whatever we're willing to add via native DLLs. Knowing the current
  ceiling tells us the size of the problem.

## Capabilities available as of 2026-04-05

### REFramework (praydog)

- Current release line: **v1.5.x** for DD2 (verified via Nexus + SourceForge
  mirrors). Actively maintained 2 years post-launch.
- Install is a single DLL drop (`dinput8.dll`) + UI via Insert key.
- Exposes a **Lua scripting API** on top of the RE Engine's IL2CPP, including:
  UI (ImGui), file I/O, access to game types/methods, event hooks on
  engine update/draw.
- Critically: **Lua scripts can require native DLLs**. This is the escape
  hatch for anything REFramework doesn't natively expose — networking,
  cryptography, persistent storage.
- No documented native network socket API in the Lua surface. We bring our
  own via a DLL addon, or pipe through a sidecar process.

### Community ecosystem

- **Nexus Mods**: ~1000+ DD2 mods as of April 2026.
- **Fluffy Mod Manager** is the de facto installer; most content ships as
  PAK files copied into its mod directory.
- **UDD2P** (Unofficial DD2 Patch) is actively maintained (last update
  2026-04-03). Worth treating as the compat baseline for mod coexistence.
- Existing mods cluster around: cosmetic, QoL, balance, camera, difficulty,
  vocation overhauls. **No dedicated multiplayer mod currently exists.**

### DD2-specific surfaces worth investigating

- **Pawn system** — already has a Capcom-hosted network-sharing mechanism
  for cross-player pawns. Hook surface? Protocol already reverse-engineered?
  (Q-0007 when opened.)
- **Save file** — sidecar-only per `client.save-file-is-sacred`; the pawn
  network path is the precedent we'd lean on.
- **Anti-cheat** — DD2 shipped without kernel anticheat. Capcom has
  retrofitted EAC to other titles; we treat `client.no-anticheat-today` as
  **soft** and watch patches.

## Sources

- REFramework: https://github.com/praydog/REFramework (repo)
- DD2 REFramework Nexus: https://www.nexusmods.com/dragonsdogma2/mods/8
- REFramework docs: https://cursey.github.io/reframework-book/
- gibbed's DD2 REFramework scripts: https://github.com/gibbed/DD2-REFramework-Scripts
- Nexus DD2 top mods: https://www.nexusmods.com/dragonsdogma2/mods/top
- UDD2P on Nexus (referenced via search, 2026-04-03 update)

## Implied implementation path

1. **Transport**: native DLL required by a REFramework Lua script. Steam GNS
   binding preferred (pattern matches `pat.p2p-via-platform-sockets`), relay
   fallback per ADR-0003.
2. **State read**: Lua/IL2CPP access to pawn position, HP, world events.
3. **State write**: ghost rendering via REFramework's draw hooks + ImGui;
   no writes to game state in v0.1 (matches `pat.ghost-before-state`).
4. **Packaging**: ship as Fluffy-compatible bundle (PAK + Lua + native DLL).
5. **Compat**: test coexistence with UDD2P and top-5 Nexus mods each release.

## Open sub-questions

- Is there a documented pawn-network RPC we can piggyback on? (Q-0007)
- What Lua→native ABI does REFramework expect? (Q-0008)
- Does REFramework's update hook fire on the render thread only, or also
  game-sim thread? (bears on `client.re-engine-single-threaded-scripting`)
