//! REFramework native plugin for dragons-frogma.
//!
//! This compiles to a `cdylib`. On Windows with the msvc toolchain it
//! produces `frogma_plugin.dll`, which REFramework loads from
//! `<DD2>/reframework/plugins/`. REFramework calls the two exported
//! entry points below; we use them to stand up a UDP peer loop and
//! register Lua-callable functions via the `on_lua_state_created`
//! callback.
//!
//! ABI mirrors REFramework's `include/reframework/API.h` at plugin
//! version 1.10.0 (the version shipping in user's DD2 REFramework
//! build, dated 2025-03-05). The functions table has 17 fields
//! (no on_pre_gui_draw_element, which was added after 1.10).
//!
//! Build (cross-compile from Linux):
//!     cargo xwin build --release -p frogma-plugin \
//!         --target x86_64-pc-windows-msvc
//!
//! Build (native Windows):
//!     cargo build --release -p frogma-plugin \
//!         --target x86_64-pc-windows-msvc
//!
//! Deploy: drop `frogma_plugin.dll` into
//!     <DD2>/reframework/plugins/
//! and pair with `reframework/autorun/frogma.lua`.

#![allow(non_camel_case_types)]

use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use std::sync::{Arc, Mutex, OnceLock};

use frogma_peer::{LocalState, PeerConfig, PeerHandle, PeerTable};

// ---------- REFramework ABI (v1.10.0) -----------------------------------

const REFRAMEWORK_PLUGIN_VERSION_MAJOR: c_int = 1;
const REFRAMEWORK_PLUGIN_VERSION_MINOR: c_int = 10;
const REFRAMEWORK_PLUGIN_VERSION_PATCH: c_int = 0;

#[repr(C)]
pub struct REFrameworkPluginVersion {
    pub major: c_int,
    pub minor: c_int,
    pub patch: c_int,
    pub game_name: *const c_char,
}

#[repr(C)]
pub struct REFrameworkRendererData {
    pub renderer_type: c_int,
    pub device: *mut c_void,
    pub swapchain: *mut c_void,
    pub command_queue: *mut c_void,
}

/// Callback types for Lua state lifecycle.
type LuaState = *mut c_void;
type REFLuaStateCreatedCb = unsafe extern "C" fn(LuaState);
type REFOnLuaStateCreatedFn =
    unsafe extern "C" fn(cb: REFLuaStateCreatedCb) -> bool;

/// Plugin-functions table, 17 fields matching API v1.10.0.
#[repr(C)]
pub struct REFrameworkPluginFunctions {
    pub on_lua_state_created: Option<REFOnLuaStateCreatedFn>,
    pub on_lua_state_destroyed: *const c_void,
    pub on_present: *const c_void,
    pub on_pre_application_entry: *const c_void,
    pub on_post_application_entry: *const c_void,
    pub lock_lua: *const c_void,
    pub unlock_lua: *const c_void,
    pub on_device_reset: *const c_void,
    pub on_message: *const c_void,
    pub log_error: Option<unsafe extern "C" fn(*const c_char)>,
    pub log_warn: Option<unsafe extern "C" fn(*const c_char)>,
    pub log_info: Option<unsafe extern "C" fn(*const c_char)>,
    pub is_drawing_ui: *const c_void,
    pub create_script_state: *const c_void,
    pub delete_script_state: *const c_void,
    pub on_imgui_frame: *const c_void,
    pub on_imgui_draw_ui: *const c_void,
    // No on_pre_gui_draw_element — that's in ≥1.11.
}

#[repr(C)]
pub struct REFrameworkPluginInitializeParam {
    pub reframework_module: *mut c_void,
    pub version: *const REFrameworkPluginVersion,
    pub functions: *const REFrameworkPluginFunctions,
    pub renderer_data: *const REFrameworkRendererData,
    pub sdk: *const c_void,
}

// ---------- Plugin state ------------------------------------------------

struct PluginState {
    peer_handle: Option<PeerHandle>,
    peer_table: Option<Arc<PeerTable>>,
    local_state: Arc<Mutex<LocalState>>,
    peer_id: u64,
    functions: Option<&'static REFrameworkPluginFunctions>,
}

static STATE: OnceLock<Mutex<PluginState>> = OnceLock::new();

fn state() -> &'static Mutex<PluginState> {
    STATE.get_or_init(|| {
        Mutex::new(PluginState {
            peer_handle: None,
            peer_table: None,
            local_state: Arc::new(Mutex::new(LocalState {
                pos: [0.0, 0.0, 0.0],
                yaw: 0.0,
                hp: 1,
                hp_max: 1,
                vocation: 0,
                pose: 0,
            })),
            peer_id: 0,
            functions: None,
        })
    })
}

fn log_info(msg: &[u8]) {
    if let Ok(guard) = state().lock() {
        if let Some(f) = guard.functions {
            if let Some(fp) = f.log_info {
                unsafe { fp(msg.as_ptr() as *const c_char) };
            }
        }
    }
}

fn log_error(msg: &[u8]) {
    if let Ok(guard) = state().lock() {
        if let Some(f) = guard.functions {
            if let Some(fp) = f.log_error {
                unsafe { fp(msg.as_ptr() as *const c_char) };
            }
        }
    }
}

// ---------- Config loading (reframework/frogma.toml) --------------------
//
// Minimal line-by-line parser. No TOML crate — the config is flat:
//
//   bind = "0.0.0.0:45100"
//   peers = ["10.0.0.2:45100", "10.0.0.3:45100"]
//
// If the file is missing or unparseable, defaults apply.

const CONFIG_PATH: &str = "reframework/frogma.toml";

struct FrogmaConfig {
    bind: std::net::SocketAddr,
    peers: Vec<std::net::SocketAddr>,
}

fn load_config() -> FrogmaConfig {
    let default = FrogmaConfig {
        bind: "0.0.0.0:45100".parse().unwrap(),
        peers: vec![],
    };

    let text = match std::fs::read_to_string(CONFIG_PATH) {
        Ok(t) => t,
        Err(_) => {
            log_info(b"[frogma] no frogma.toml found - using defaults\0");
            return default;
        }
    };

    let mut bind = default.bind;
    let mut peers = vec![];

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(val) = line.strip_prefix("bind") {
            let val = val.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
            let val = val.trim_end_matches('"').trim();
            if let Ok(addr) = val.parse() {
                bind = addr;
            }
        } else if let Some(val) = line.strip_prefix("peers") {
            // Parse: peers = ["addr1", "addr2"]
            let val = val.trim_start_matches(|c: char| c == ' ' || c == '=');
            // Extract quoted strings from the bracketed list.
            for part in val.split('"') {
                let part = part.trim();
                if let Ok(addr) = part.parse::<std::net::SocketAddr>() {
                    peers.push(addr);
                }
            }
        }
    }

    FrogmaConfig { bind, peers }
}

// ---------- REFramework entry points ------------------------------------

#[no_mangle]
pub unsafe extern "C" fn reframework_plugin_required_version(
    version: *mut REFrameworkPluginVersion,
) -> bool {
    if version.is_null() {
        return false;
    }
    (*version).major = REFRAMEWORK_PLUGIN_VERSION_MAJOR;
    (*version).minor = REFRAMEWORK_PLUGIN_VERSION_MINOR;
    (*version).patch = REFRAMEWORK_PLUGIN_VERSION_PATCH;
    (*version).game_name = GAME_NAME_DD2.as_ptr() as *const c_char;
    true
}

const GAME_NAME_DD2: &[u8] = b"DD2\0";

#[no_mangle]
pub unsafe extern "C" fn reframework_plugin_initialize(
    param: *const REFrameworkPluginInitializeParam,
) -> bool {
    if param.is_null() {
        return false;
    }
    let p = &*param;

    {
        let mut guard = state().lock().unwrap();
        guard.functions = if p.functions.is_null() {
            None
        } else {
            Some(&*p.functions)
        };
        guard.peer_id = fresh_peer_id();
    }

    log_info(b"[frogma] plugin_initialize: starting peer loop\0");

    log_info(b"[frogma] using file-based IPC (frogma_peers.txt / frogma_local.txt)\0");

    let peer_id = state().lock().unwrap().peer_id;
    let config = load_config();
    // Log loaded config (can't use format! with log_info, so use fixed messages).
    if config.peers.is_empty() {
        log_info(b"[frogma] config: no peers - broadcasting to nobody until frogma.toml is edited\0");
    } else {
        log_info(b"[frogma] config: peers loaded from frogma.toml\0");
    }
    let cfg = PeerConfig {
        peer_id,
        bind: config.bind,
        peers: config.peers,
        tick: std::time::Duration::from_millis(100),
    };

    let shared = state().lock().unwrap().local_state.clone();
    let provider: frogma_peer::StateProvider = Box::new(move || *shared.lock().unwrap());

    match frogma_peer::start(cfg, provider) {
        Ok(handle) => {
            let table = handle.table.clone();
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let local_for_ipc = state().lock().unwrap().local_state.clone();
            start_ipc_bridge(table.clone(), local_for_ipc, peer_id, stop);

            let mut guard = state().lock().unwrap();
            guard.peer_table = Some(table);
            guard.peer_handle = Some(handle);
            drop(guard);
            log_info(b"[frogma] plugin_initialize: peer loop up on 0.0.0.0:45100\0");
            true
        }
        Err(_) => {
            log_error(b"[frogma] plugin_initialize: frogma_peer::start failed\0");
            false
        }
    }
}

fn fresh_peer_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdead_beef)
}

// ---------- File-based IPC bridge ---------------------------------------
//
// REFramework statically links LuaJIT without exporting the Lua C API,
// and Lua 5.1 C API is NOT binary-compatible with LuaJIT's lua_State
// layout. So we can't register Lua functions from the plugin.
//
// Instead we use text files for bidirectional IPC:
//   - Plugin writes  reframework/frogma_peers.txt  (peer snapshots)
//   - Lua writes     reframework/frogma_local.txt  (local player state)
//   - Plugin reads   frogma_local.txt to feed the tx thread
//   - Lua reads      frogma_peers.txt to draw ghost markers
//
// Both files are tiny (<1 KB), written atomically via rename, and
// polled at frame rate. This is architecturally ugly but mechanically
// simple and completely avoids the Lua C API problem.

/// IPC file paths relative to DD2's working directory.
const PEERS_PATH: &str = "reframework/frogma_peers.txt";
const LOCAL_PATH: &str = "reframework/frogma_local.txt";
const PEER_ID_PATH: &str = "reframework/frogma_peer_id.txt";

/// Start a background thread that:
/// 1. Writes peer snapshots to PEERS_PATH every 100ms
/// 2. Reads local player state from LOCAL_PATH every 100ms
fn start_ipc_bridge(
    peer_table: Arc<PeerTable>,
    local_state: Arc<Mutex<LocalState>>,
    peer_id: u64,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::io::Write;

    // Write peer_id once so Lua can identify us.
    if let Ok(mut f) = std::fs::File::create(PEER_ID_PATH) {
        let _ = writeln!(f, "{peer_id}");
    }

    std::thread::Builder::new()
        .name("frogma-ipc".into())
        .spawn(move || {
            let mut buf = String::with_capacity(512);
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                // --- Write peers ---
                let snaps = peer_table.snapshot();
                buf.clear();
                buf.push_str(&format!("{}\n", snaps.len()));
                for (_, s) in &snaps {
                    buf.push_str(&format!(
                        "{} {} {} {} {} {} {} {} {}\n",
                        s.peer_id,
                        s.pos[0], s.pos[1], s.pos[2],
                        s.yaw, s.hp, s.hp_max, s.vocation, s.pose,
                    ));
                }
                // Atomic write via temp + rename.
                let tmp = format!("{PEERS_PATH}.tmp");
                if let Ok(mut f) = std::fs::File::create(&tmp) {
                    let _ = f.write_all(buf.as_bytes());
                    let _ = std::fs::rename(&tmp, PEERS_PATH);
                }

                // --- Read local player state ---
                if let Ok(text) = std::fs::read_to_string(LOCAL_PATH) {
                    let nums: Vec<f64> = text
                        .split_whitespace()
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if nums.len() >= 8 {
                        *local_state.lock().unwrap() = LocalState {
                            pos: [nums[0] as f32, nums[1] as f32, nums[2] as f32],
                            yaw: nums[3] as f32,
                            hp: nums[4] as u16,
                            hp_max: nums[5] as u16,
                            vocation: nums[6] as u8,
                            pose: nums[7] as u8,
                        };
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .ok();
}

// Safety: REFramework owns the pointees for the plugin's lifetime.
unsafe impl Send for REFrameworkPluginInitializeParam {}
unsafe impl Sync for REFrameworkPluginInitializeParam {}
unsafe impl Send for REFrameworkPluginFunctions {}
unsafe impl Sync for REFrameworkPluginFunctions {}
unsafe impl Send for REFrameworkRendererData {}
unsafe impl Sync for REFrameworkRendererData {}

// ---------- Internal helpers (used by tests and IPC) --------------------

fn frogma_push_local_state(
    pos_x: f32, pos_y: f32, pos_z: f32, yaw: f32,
    hp: u16, hp_max: u16, vocation: u8, pose: u8,
) {
    let local = state().lock().unwrap().local_state.clone();
    *local.lock().unwrap() = LocalState {
        pos: [pos_x, pos_y, pos_z],
        yaw, hp, hp_max, vocation, pose,
    };
}

fn frogma_peer_count() -> usize {
    match &state().lock().unwrap().peer_table {
        Some(t) => t.len(),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_local_state_updates_shared() {
        frogma_push_local_state(1.5, 2.5, 3.5, 0.75, 100, 200, 3, 1);
        let s = state().lock().unwrap().local_state.clone();
        let st = *s.lock().unwrap();
        assert_eq!(st.pos, [1.5, 2.5, 3.5]);
        assert_eq!(st.yaw, 0.75);
        assert_eq!(st.hp, 100);
        assert_eq!(st.hp_max, 200);
        assert_eq!(st.vocation, 3);
        assert_eq!(st.pose, 1);
    }

    #[test]
    fn peer_count_is_zero_before_init() {
        assert_eq!(frogma_peer_count(), 0);
    }
}
