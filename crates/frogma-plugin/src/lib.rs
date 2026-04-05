//! REFramework native plugin stub.
//!
//! This compiles to a `cdylib`. On Windows with the msvc toolchain it
//! produces `frogma_plugin.dll`, which is what REFramework loads from
//! `reframework/plugins/`. REFramework calls `reframework_plugin_required_version`
//! and `reframework_plugin_initialize`; we keep both small.
//!
//! This file cannot be exercised in the Linux sandbox — it only compiles
//! against the reframework plugin headers' ABI in spirit. But the
//! compile-time cdylib check + the peer loop it wraps are both testable
//! here. The actual DD2-side integration lives on Windows.
//!
//! Build on Windows (or via x-compile):
//!     rustup target add x86_64-pc-windows-msvc
//!     cargo build --release -p frogma-plugin --target x86_64-pc-windows-msvc
//!
//! Deploy: drop `frogma_plugin.dll` into
//!     <DD2>/reframework/plugins/frogma_plugin.dll
//! and pair with `reframework/autorun/frogma.lua` (elsewhere in this repo).

use std::ffi::c_void;
use std::sync::Mutex;

use frogma_peer::{PeerConfig, PeerHandle};

/// REFramework expects this exact function name.
/// The return value is the minimum plugin API version we require.
/// We target the lowest useful version; REFramework bumps this rarely.
#[no_mangle]
pub extern "C" fn reframework_plugin_required_version(
    version: *mut REFrameworkPluginVersion,
) -> bool {
    // SAFETY: REFramework guarantees `version` points to a writable struct.
    if version.is_null() {
        return false;
    }
    unsafe {
        (*version).major = 1;
        (*version).minor = 0;
        (*version).patch = 0;
        (*version).game_name = b"DD2\0".as_ptr() as *const i8;
    }
    true
}

/// Called once after REFramework loads us. We take the plugin parameters
/// (containing function pointers into REFramework), spin up the peer
/// thread, and store the handle so it outlives this call.
#[no_mangle]
pub extern "C" fn reframework_plugin_initialize(
    _param: *const REFrameworkPluginInitializeParam,
) -> bool {
    // Peer config will come from a sidecar toml in later iterations.
    // For the stub we hardcode a dev config that a co-located Lua script
    // can override by calling back into us via Lua bindings.
    let cfg = PeerConfig {
        peer_id: fresh_peer_id(),
        bind: "0.0.0.0:45100".parse().unwrap(),
        peers: vec![], // populated from Lua at runtime
        tick: std::time::Duration::from_millis(100),
    };

    let provider: frogma_peer::StateProvider = Box::new(|| frogma_peer::LocalState {
        pos: [0.0, 0.0, 0.0],
        yaw: 0.0,
        hp: 1,
        hp_max: 1,
        vocation: 0,
        pose: 0,
    });

    match frogma_peer::start(cfg, provider) {
        Ok(handle) => {
            *HANDLE.lock().unwrap() = Some(handle);
            true
        }
        Err(_) => false,
    }
}

static HANDLE: Mutex<Option<PeerHandle>> = Mutex::new(None);

fn fresh_peer_id() -> u64 {
    // Not cryptographic. Random-ish startup id is fine per ADR-0002.
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdead_beef)
}

// ---------- REFramework ABI (minimal subset) ---------------------------
//
// These are placeholder shapes. The real definitions live in REFramework's
// plugin.h header. For the spike we only need the two entry points above
// to link; the struct layouts here are not exercised at runtime until we
// cross-compile and REFramework actually calls us.

#[repr(C)]
pub struct REFrameworkPluginVersion {
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub game_name: *const i8,
}

#[repr(C)]
pub struct REFrameworkPluginInitializeParam {
    // REFramework passes a big struct of function pointers here. Opaque
    // until we actually wire into a specific REFramework SDK release.
    pub _opaque: *const c_void,
}

// Safety: REFramework is single-threaded into this callback; the peer
// threads we spawn are independent.
unsafe impl Send for REFrameworkPluginInitializeParam {}
unsafe impl Sync for REFrameworkPluginInitializeParam {}
