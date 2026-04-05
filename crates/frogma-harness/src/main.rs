//! Loopback harness: spins up N frogma-peer instances on different
//! ports, each broadcasting to the others. Prints a table of observed
//! peers every tick. Exits after a fixed duration.
//!
//! This validates the transport layer end-to-end without REFramework
//! or DD2. If this works, the UDP + ring-buffer + seq logic is sound;
//! all that remains is dropping the peer loop behind a REFramework
//! plugin entry point.

use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use frogma_peer::{start, LocalState, PeerConfig};

const N_PEERS: usize = 3;
const RUN_FOR: Duration = Duration::from_secs(3);
const TICK: Duration = Duration::from_millis(100); // 10 Hz

fn main() {
    let base_port = 45_100u16;
    let peers: Vec<SocketAddr> = (0..N_PEERS)
        .map(|i| format!("127.0.0.1:{}", base_port + i as u16).parse().unwrap())
        .collect();

    let mut handles = Vec::new();
    for i in 0..N_PEERS {
        let peer_id = 0xA0_u64 | (i as u64);
        let bind = peers[i];
        // Broadcast to everyone else (unicast loop).
        let mut others: Vec<SocketAddr> = peers.clone();
        others.remove(i);

        let cfg = PeerConfig {
            peer_id,
            bind,
            peers: others,
            tick: TICK,
        };

        // Each peer walks in a circle so motion is observable.
        let start_t = Instant::now();
        let i_f = i as f32;
        let provider: frogma_peer::StateProvider = Box::new(move || {
            let t = start_t.elapsed().as_secs_f32();
            LocalState {
                pos: [
                    (t + i_f * 2.0).cos() * 10.0,
                    0.0,
                    (t + i_f * 2.0).sin() * 10.0,
                ],
                yaw: t,
                hp: 1000 - (i as u16 * 50),
                hp_max: 1000,
                vocation: (i as u8) + 1,
                pose: 0,
            }
        });

        let handle = start(cfg, provider).expect("peer start");
        handles.push((peer_id, handle));
    }

    println!("frogma-harness: {N_PEERS} peers, tick={:?}, running {:?}", TICK, RUN_FOR);
    println!();

    let t0 = Instant::now();
    let mut last_print = Instant::now();
    while t0.elapsed() < RUN_FOR {
        thread::sleep(Duration::from_millis(500));
        if last_print.elapsed() >= Duration::from_millis(500) {
            last_print = Instant::now();
            println!("t={:>4.1}s", t0.elapsed().as_secs_f32());
            for (id, h) in &handles {
                let observed = h.table.snapshot();
                print!("  peer {:#04x} sees {} others: ", id, observed.len());
                for (other_id, s) in &observed {
                    print!("[{:#04x} seq={} pos=({:+.1},{:+.1},{:+.1})] ",
                        other_id, s.seq, s.pos[0], s.pos[1], s.pos[2]);
                }
                println!();
            }
        }
    }

    println!("\nshutting down");
    for (id, h) in handles {
        let observed = h.table.snapshot();
        h.stop();
        println!("  peer {:#04x} final observed {} others", id, observed.len());
    }
}
