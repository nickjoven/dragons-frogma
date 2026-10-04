# ADR-0003: Rendering — ghost overlays via REFramework draw hook

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001

## Decision

Friends' avatars are rendered as **overlays**, not as entities inside the
DD2 game world. The mod never spawns objects, mutates entities, or writes
to anything the engine will persist. We draw on top of the frame.

Per-frame, for each remote peer with a recent snapshot:
1. Take the peer's interpolated world-space position.
2. Project it to screen space using the current camera matrix
   (read via REFramework's IL2CPP surface).
3. Draw a small marker (billboard sprite or ImGui-in-world overlay) with
   optional name/HP bar.

If `reframework-d2d` or equivalent is available, use it for the draw.
Otherwise fall back to ImGui overlays positioned via screen projection.

## What we read (IL2CPP surface, read-only)

- Player camera transform (position, rotation, FOV, view/proj matrices)
- Local player position (to compute distance for fade / culling)
- Frame / tick time for interpolation

We do not touch: entities, pawn AI, quest flags, inventory, saves.

## Why overlays, not in-world entities

- In-world entities would have to be spawned/managed through game APIs
  we don't fully understand → `client.patch-breaks-offsets` risk
  multiplies.
- Overlays are purely additive: worst case, they draw in the wrong place.
  They cannot corrupt the game.
- Honors `inv.no-game-state-writes` trivially.

## Open questions

- Q-0002: what IL2CPP methods / types expose the camera matrices and
  local player position reliably?
- Q-0003: does REFramework's render hook fire on a thread where GPU
  resources are safe to touch (the game's render thread), or does it
  need marshalling?

## Known constraints that pressure this

- `client.re-engine-single-threaded-scripting` — Lua runs on render
  thread; expensive draw loops will jitter the frame.
- `client.patch-breaks-offsets` — any IL2CPP type/method we rely on
  needs a signature check after DD2 patches.
