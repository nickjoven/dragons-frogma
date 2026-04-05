# Environmental substrate: what's possible, what isn't

This is the ground truth the architecture stands on. Each constraint here has
a CID in the knowledge graph and is linked to the ADR(s) it forced. If a
constraint is later shown to be false, supersede it — and the ADRs that
depended on it go up for review automatically via `ket_children`.

Constraints are grouped by domain, each with kind:
- **hard** — physical or legal; treat as axiom
- **soft** — true today, tunable or negotiable
- **economic** — true under current cost assumptions
- **observed** — verified in this repo's history

## A. Development & build environment (observed this session)

| id | statement | kind |
|----|-----------|------|
| `env.mcp-loads-at-session-start` | MCP servers register at Claude Code session start; mid-session `.mcp.json` changes require restart. | observed |
| `env.ket-store-nondeterministic-node-cid` | `ket_store` returns stable `content_cid` but fresh `node_cid` each call (timestamp in header). Lineage equality must use content_cid. | observed |
| `env.cargo-needs-network` | First-time builds fetch git deps (`ket`, `canon.d`) + crates.io; CI must allow outbound HTTPS. | observed |
| `env.cloud-sandbox-ephemeral` | The dev cloud sandbox resets between sessions; `.ket/` persists only via git-ignored local dir or explicit export. | observed |
| `env.github-mcp-can-disconnect` | External MCP servers (github) may drop mid-session without warning. Don't treat tool availability as static. | observed |

## B. Dragon's Dogma 2 client environment

| id | statement | kind |
|----|-----------|------|
| `client.no-asset-redistribution` | We cannot ship any DD2 textures, meshes, audio, scripts, or decompiled code. | hard |
| `client.reframework-dependency` | Hooking DD2 is practical only via REFramework / Lua; direct memory patching is brittle across Capcom patches. | soft |
| `client.patch-breaks-offsets` | Capcom patches rotate struct layouts; any offset-based hook needs a versioned signature table. | hard |
| `client.re-engine-single-threaded-scripting` | REFramework Lua runs on the render thread; heavy I/O must be off-thread. | hard |
| `client.no-anticheat-today` | DD2 ships without kernel anti-cheat as of this writing, but that can change in a patch. | soft |
| `client.save-file-is-sacred` | Never write to the player's DD2 save file. All mod state lives in a sidecar. | hard |

## C. Distribution & legal

| id | statement | kind |
|----|-----------|------|
| `dist.steam-workshop-gated` | DD2 Workshop is Capcom-moderated; assume network-code mods are disallowed. Distribute via GitHub releases + Nexus. | soft |
| `dist.no-rights-reserved` | Mod license is permissive (MIT-compatible). No EULA overlay, no telemetry without explicit opt-in. | hard |
| `dist.dmca-surface` | A relay that stores arbitrary player-authored content has DMCA exposure. Keep relays dumb, CID-only, TTL-bounded. | hard |
| `dist.eu-data-residency` | Player-identifying payloads (even nicknames) cross GDPR. Treat player IDs as pseudonymous keys. | hard |

## D. Network & transport

| id | statement | kind |
|----|-----------|------|
| `net.p2p-nat-hostile` | Most players are behind symmetric NAT; pure P2P requires relay fallback anyway. Start with relay-only. | hard |
| `net.offline-days-common` | Players frequently play offline for days. Inbox must tolerate arbitrary drain delay. | hard |
| `net.mobile-backhaul` | Some players tether; bandwidth budget per session should be < 1 MB/hour baseline. | economic |
| `net.relay-cheap-but-not-free` | A single $5/mo VPS can fan out ~10k CIDs/day; beyond that needs sharding or BYO-relay. | economic |
| `net.no-server-authoritative-physics` | We cannot and will not replicate DD2's physics server-side. Projections are best-effort. | hard |

## E. Operational

| id | statement | kind |
|----|-----------|------|
| `ops.solo-maintainer` | Assume one maintainer. Anything that needs 24/7 ops is out of scope. | economic |
| `ops.no-paid-infra` | MVP runs on a single donated VPS. Scale-out is a later problem. | economic |
| `ops.telemetry-opt-in-only` | No crash reports, no metrics, no pings without the player clicking yes. | hard |

## How constraints shape the architecture

- `net.p2p-nat-hostile` + `net.offline-days-common` → **ADR-0003 (async relays)**
- `client.no-asset-redistribution` + `dist.dmca-surface` → **ADR-0002 (CID-only relays)**
- `net.no-server-authoritative-physics` + `ops.solo-maintainer` → **ADR-0005 (Lagrangian relaxation)**
- `client.patch-breaks-offsets` → forces a versioned hook table (future ADR-0006)
- `env.ket-store-nondeterministic-node-cid` → `cids.lock` pins **content_cid**, not node_cid
