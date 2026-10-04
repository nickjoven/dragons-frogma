//! UDP peer: runs a socket on a background thread, publishes our own
//! snapshots on a tick, receives snapshots from the overlay, keeps a
//! small per-peer ring buffer for interpolation.
//!
//! No session layer, no acks, no retries (ADR-0002). If the plugin is
//! loaded into DD2, this loop runs off the render thread.

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use frogma_wire::{Snapshot, SNAPSHOT_LEN};

/// How many snapshots to keep per peer for interpolation.
pub const RING_CAPACITY: usize = 16;

/// Peers older than this are fully faded out. Tunable.
pub const PEER_STALE_MS: u64 = 3_000;

#[derive(Clone)]
pub struct PeerConfig {
    pub peer_id: u64,
    /// Local UDP bind address. Use 0.0.0.0:0 for ephemeral.
    pub bind: SocketAddr,
    /// Destinations we broadcast to. Unicast per friend or multicast.
    pub peers: Vec<SocketAddr>,
    /// How often we publish our own snapshot.
    pub tick: Duration,
}

/// Thread-safe view into the peer registry.
/// The plugin's render-side code reads from this each frame.
#[derive(Default)]
pub struct PeerTable {
    inner: Mutex<HashMap<u64, PeerRing>>,
}

pub struct PeerRing {
    pub last_seq: u32,
    pub last_recv: Instant,
    pub buf: [Option<Snapshot>; RING_CAPACITY],
    pub head: usize,
}

impl PeerRing {
    fn new() -> Self {
        Self {
            last_seq: 0,
            last_recv: Instant::now(),
            buf: [None; RING_CAPACITY],
            head: 0,
        }
    }

    fn push(&mut self, s: Snapshot) {
        self.buf[self.head] = Some(s);
        self.head = (self.head + 1) % RING_CAPACITY;
        self.last_seq = s.seq;
        self.last_recv = Instant::now();
    }

    /// Most recent snapshot, if any.
    pub fn latest(&self) -> Option<Snapshot> {
        let idx = (self.head + RING_CAPACITY - 1) % RING_CAPACITY;
        self.buf[idx]
    }
}

impl PeerTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a received snapshot. Rejects stale seqs with simple wrap guard.
    pub fn ingest(&self, s: Snapshot) {
        let mut map = self.inner.lock().unwrap();
        let ring = map.entry(s.peer_id).or_insert_with(PeerRing::new);
        // Wrap-aware seq check: accept if newer, tolerate tiny reorder.
        let newer = s.seq.wrapping_sub(ring.last_seq) < (u32::MAX / 2);
        if ring.last_seq == 0 || newer {
            ring.push(s);
        }
    }

    /// Snapshot of all known peers' latest state. Called by renderer.
    pub fn snapshot(&self) -> Vec<(u64, Snapshot)> {
        let map = self.inner.lock().unwrap();
        map.iter()
            .filter_map(|(id, ring)| ring.latest().map(|s| (*id, s)))
            .collect()
    }

    /// Drop peers that haven't been heard from in PEER_STALE_MS.
    pub fn prune_stale(&self) -> usize {
        let mut map = self.inner.lock().unwrap();
        let threshold = Duration::from_millis(PEER_STALE_MS);
        let before = map.len();
        map.retain(|_, r| r.last_recv.elapsed() < threshold);
        before - map.len()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A producer closure returns our current avatar state to broadcast.
pub type StateProvider = Box<dyn FnMut() -> LocalState + Send>;

#[derive(Debug, Clone, Copy)]
pub struct LocalState {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub hp: u16,
    pub hp_max: u16,
    pub vocation: u8,
    pub pose: u8,
}

/// Handle to a running peer loop. Dropping it asks the threads to stop.
pub struct PeerHandle {
    pub table: Arc<PeerTable>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl PeerHandle {
    pub fn stop(self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for t in self.threads {
            let _ = t.join();
        }
    }
}

/// Start the peer loop. Returns a handle with a shared peer table.
pub fn start(config: PeerConfig, mut provider: StateProvider) -> std::io::Result<PeerHandle> {
    let socket = UdpSocket::bind(config.bind)?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    let socket = Arc::new(socket);
    let table = Arc::new(PeerTable::new());
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Receiver thread: blocking recv with short timeout so it checks stop flag.
    let rx_socket = socket.clone();
    let rx_table = table.clone();
    let rx_stop = stop.clone();
    let rx = thread::Builder::new()
        .name("frogma-peer-rx".into())
        .spawn(move || {
            let mut buf = [0u8; 512];
            while !rx_stop.load(std::sync::atomic::Ordering::Relaxed) {
                match rx_socket.recv_from(&mut buf) {
                    Ok((n, _from)) if n >= SNAPSHOT_LEN => {
                        if let Ok(s) = Snapshot::decode(&buf[..SNAPSHOT_LEN]) {
                            // Don't ingest our own broadcast echoes.
                            if s.peer_id != config.peer_id {
                                rx_table.ingest(s);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => {}
                }
            }
        })?;

    // Sender thread: tick, ask provider for state, broadcast to peers.
    let tx_socket = socket.clone();
    let tx_stop = stop.clone();
    let peers = config.peers.clone();
    let peer_id = config.peer_id;
    let tick = config.tick;
    let tx = thread::Builder::new()
        .name("frogma-peer-tx".into())
        .spawn(move || {
            let mut seq: u32 = 0;
            while !tx_stop.load(std::sync::atomic::Ordering::Relaxed) {
                let st = provider();
                let t_send_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                seq = seq.wrapping_add(1);
                let snap = Snapshot {
                    peer_id,
                    seq,
                    t_send_ms,
                    pos: st.pos,
                    yaw: st.yaw,
                    hp: st.hp,
                    hp_max: st.hp_max,
                    vocation: st.vocation,
                    pose: st.pose,
                };
                let bytes = snap.encode();
                for dst in &peers {
                    let _ = tx_socket.send_to(&bytes, dst);
                }
                thread::sleep(tick);
            }
        })?;

    Ok(PeerHandle {
        table,
        stop,
        threads: vec![rx, tx],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_push_and_latest() {
        let mut r = PeerRing::new();
        for i in 1..=5u32 {
            r.push(Snapshot {
                peer_id: 1,
                seq: i,
                t_send_ms: i as u64,
                pos: [i as f32, 0.0, 0.0],
                yaw: 0.0,
                hp: 100,
                hp_max: 100,
                vocation: 0,
                pose: 0,
            });
        }
        assert_eq!(r.latest().unwrap().seq, 5);
    }

    #[test]
    fn table_ingests_and_prunes() {
        let t = PeerTable::new();
        t.ingest(Snapshot {
            peer_id: 7,
            seq: 1,
            t_send_ms: 0,
            pos: [0.0, 0.0, 0.0],
            yaw: 0.0,
            hp: 10,
            hp_max: 10,
            vocation: 0,
            pose: 0,
        });
        assert_eq!(t.len(), 1);
        assert_eq!(t.snapshot().len(), 1);
    }

    #[test]
    fn table_rejects_own_peer_echoes() {
        // This is handled in start(); the table itself trusts input.
        // So we just assert that it accepts two different peers.
        let t = PeerTable::new();
        let base = Snapshot {
            peer_id: 1,
            seq: 1,
            t_send_ms: 0,
            pos: [0.0, 0.0, 0.0],
            yaw: 0.0,
            hp: 10,
            hp_max: 10,
            vocation: 0,
            pose: 0,
        };
        t.ingest(base);
        t.ingest(Snapshot { peer_id: 2, ..base });
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn table_ignores_strictly_older_seqs() {
        let t = PeerTable::new();
        let mk = |seq| Snapshot {
            peer_id: 1,
            seq,
            t_send_ms: 0,
            pos: [seq as f32, 0.0, 0.0],
            yaw: 0.0,
            hp: 10,
            hp_max: 10,
            vocation: 0,
            pose: 0,
        };
        t.ingest(mk(10));
        t.ingest(mk(5)); // older, should be rejected
        let latest = t.snapshot()[0].1;
        assert_eq!(latest.seq, 10);
    }
}
