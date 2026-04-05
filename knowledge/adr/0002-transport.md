# ADR-0002: Transport — UDP state snapshots on a shared overlay

- **Status:** accepted
- **Created:** 2026-04-05
- **Parents:** ADR-0001

## Decision

A small native DLL, loaded by a REFramework Lua script, runs a UDP socket
on a background thread. Every N ms (start at 10 Hz, tune later) each peer
broadcasts a fixed-size snapshot of their avatar state. Peers unicast or
multicast to the shared overlay; there is no relay, no server, no session
concept.

State snapshots are latest-wins per peer. In-memory ring buffer ~1s deep
per peer for interpolation. Nothing is persisted across sessions.

Snapshot wire format (v0):

    peer_id         : 8 bytes (random at startup, not cryptographic identity)
    seq             : 4 bytes (monotonic counter)
    t_send_ms       : 8 bytes (sender's wall clock)
    pos_x,y,z       : 12 bytes (float32)
    yaw             : 4 bytes (float32)
    hp, hp_max      : 4 bytes (u16 each)
    vocation, pose  : 2 bytes
    ---
    total: ~42 bytes + UDP/IP overhead

## Why UDP, why no session layer

- Friends on Tailscale → no NAT traversal problem.
- Positional data is ephemeral; loss is tolerable; 5-10 Hz with small
  packets is negligible bandwidth.
- Session management (joins, leaves, party state) is hand-coordinated in
  a group chat. Software doesn't need to know.
- No reliability layer, no ordering guarantees, no retries. Late packets
  are dropped. Stale peers just fade.

## Why not Steam GNS

GNS is great for Internet-scale P2P with NAT traversal. We don't need
either. Adding GNS is 30k lines of dependency for a problem Tailscale
already solved for the friend group.

## Open questions

- Q-0001: smallest REFramework plugin DLL that can run this UDP loop
  off-thread without stalling the render pipeline.

## Explicit non-invariants (things we don't guarantee)

- Message delivery. Not guaranteed.
- Ordering. Not guaranteed.
- Peer identity authenticity. Friends trust each other.
- Clock synchronization. `t_send_ms` is best-effort, consumer-interpreted.
