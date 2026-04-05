# Constraints that actually apply

Short, focused list. Most of the distribution / DMCA / relay-economics
material was mooted by ADR-0001's pet-project scope and has been removed.
Git history has the old constraints if we ever need them back.

## Client environment

| id | statement | kind |
|----|-----------|------|
| `client.reframework-dependency` | Hooks come only from REFramework Lua + plugin DLL; no memory patching, no DLL injection outside REFramework's loader. | hard |
| `client.patch-breaks-offsets` | DD2 patches can rotate IL2CPP type layouts. Any typed access needs a signature/version check. | hard |
| `client.re-engine-single-threaded-scripting` | REFramework Lua runs on the render thread. Network I/O and any sustained work goes off-thread via the native plugin. | hard |
| `client.save-file-is-sacred` | Never read, write, or stat DD2 save files. The mod has no persistent on-disk state at all in v0.1. | hard |
| `client.no-asset-redistribution` | Never ship Capcom assets (textures, meshes, scripts, audio, decompiled code). Not even "for convenience." | hard |
| `client.no-game-state-writes` | Never mutate DD2 entities, pawns, quest flags, inventories, world state. Overlay draws only. | hard |
| `client.dont-re-capcom-netcode` | Do not reverse-engineer or hook into DD2's pawn-sharing network. Read state only via REFramework's IL2CPP surface. | hard |

## Operational

| id | statement | kind |
|----|-----------|------|
| `ops.solo-maintainer` | One person. Every architectural decision is priced in weekends. | hard |
| `ops.friends-on-overlay` | Assume friends share a Tailscale/ZeroTier tailnet or LAN. NAT traversal is not our problem. | soft |
| `ops.no-persistence` | No databases, no disk state, no migrations. In-memory ring buffers only. | soft |

## How these shape the ADRs

- `client.reframework-dependency` + `client.re-engine-single-threaded-scripting` → ADR-0002 (plugin DLL runs UDP off-thread)
- `client.no-game-state-writes` + `client.save-file-is-sacred` → ADR-0003 (overlay rendering only)
- `ops.friends-on-overlay` → ADR-0002 (no NAT traversal, no GNS)
- `client.patch-breaks-offsets` → every IL2CPP read carries a version guard
