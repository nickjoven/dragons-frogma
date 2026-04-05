//! REFramework native plugin for dragons-frogma.
//!
//! This compiles to a `cdylib`. On Windows with the msvc toolchain it
//! produces `frogma_plugin.dll`, which REFramework loads from
//! `<DD2>/reframework/plugins/`. REFramework calls the two exported
//! entry points below; we use them to stand up a UDP peer loop and
//! stash the REFramework function table for later use from the Lua
//! bridge.
//!
//! ABI mirrors REFramework's `include/reframework/API.h` at plugin
//! version 1.15.0. We keep only the fields we actually touch typed;
//! everything else is an opaque pointer so REFramework-internal
//! layout changes can't misalign us.
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

// ---------- REFramework ABI ---------------------------------------------
//
// Verbatim from praydog/REFramework `include/reframework/API.h` at
// plugin version 1.15.0.

const REFRAMEWORK_PLUGIN_VERSION_MAJOR: c_int = 1;
const REFRAMEWORK_PLUGIN_VERSION_MINOR: c_int = 15;
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
    pub renderer_type: c_int, // 0 = D3D11, 1 = D3D12
    pub device: *mut c_void,
    pub swapchain: *mut c_void,
    pub command_queue: *mut c_void,
}

/// Subset of REFramework's plugin-functions table.
/// Field order must match `REFrameworkPluginFunctions` in API.h.
#[repr(C)]
pub struct REFrameworkPluginFunctions {
    pub on_lua_state_created: *const c_void,
    pub on_lua_state_destroyed: *const c_void,
    pub on_present: *const c_void,
    pub on_pre_application_entry: *const c_void,
    pub on_post_application_entry: *const c_void,
    pub lock_lua: *const c_void,
    pub unlock_lua: *const c_void,
    pub on_device_reset: *const c_void,
    pub on_message: *const c_void,
    /// `void log_error(const char* format, ...)` — we only pass
    /// pre-formatted strings, so zero-varargs call is ABI-compatible
    /// on x86_64.
    pub log_error: Option<unsafe extern "C" fn(*const c_char)>,
    pub log_warn: Option<unsafe extern "C" fn(*const c_char)>,
    pub log_info: Option<unsafe extern "C" fn(*const c_char)>,
    pub is_drawing_ui: *const c_void,
    pub create_script_state: *const c_void,
    pub delete_script_state: *const c_void,
    pub on_imgui_frame: *const c_void,
    pub on_imgui_draw_ui: *const c_void,
    pub on_pre_gui_draw_element: *const c_void,
}

#[repr(C)]
pub struct REFrameworkPluginInitializeParam {
    pub reframework_module: *mut c_void,
    pub version: *const REFrameworkPluginVersion,
    pub functions: *const REFrameworkPluginFunctions,
    pub renderer_data: *const REFrameworkRendererData,
    /// `const REFrameworkSDKData*` — opaque for v0.
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
    let guard = state().lock().unwrap();
    if let Some(f) = guard.functions {
        if let Some(fp) = f.log_info {
            unsafe { fp(msg.as_ptr() as *const c_char) };
        }
    }
}

fn log_error(msg: &[u8]) {
    let guard = state().lock().unwrap();
    if let Some(f) = guard.functions {
        if let Some(fp) = f.log_error {
            unsafe { fp(msg.as_ptr() as *const c_char) };
        }
    }
}

// ---------- REFramework entry points ------------------------------------

/// REFramework calls this first, expecting us to fill in the minimum
/// plugin API version we were built against.
///
/// # Safety
/// REFramework guarantees `version` is a valid writable pointer.
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

/// Called once after REFramework loads us. Stashes the function
/// table, spins up the peer transport, stores the peer table
/// handle so C-ABI bridge exports can read from it.
///
/// # Safety
/// REFramework guarantees `param` is valid for the lifetime of the
/// plugin.
#[no_mangle]
pub unsafe extern "C" fn reframework_plugin_initialize(
    param: *const REFrameworkPluginInitializeParam,
) -> bool {
    if param.is_null() {
        return false;
    }
    let p = &*param;

    // Stash functions table first so log_info works everywhere below.
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

    let peer_id = state().lock().unwrap().peer_id;
    let cfg = PeerConfig {
        peer_id,
        bind: "0.0.0.0:45100".parse().unwrap(),
        peers: vec![], // populated from Lua / config sidecar (Leg F)
        tick: std::time::Duration::from_millis(100),
    };

    // StateProvider reads from our shared Arc<Mutex<LocalState>>
    // that Lua updates via frogma_push_local_state each frame.
    let shared = state().lock().unwrap().local_state.clone();
    let provider: frogma_peer::StateProvider = Box::new(move || *shared.lock().unwrap());

    match frogma_peer::start(cfg, provider) {
        Ok(handle) => {
            let mut guard = state().lock().unwrap();
            guard.peer_table = Some(handle.table.clone());
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

// ---------- C ABI bridge for Lua ----------------------------------------
//
// Called from reframework/autorun/frogma.lua via package.loadlib or
// REFramework's native-function hooks. All exports are `#[no_mangle]
// extern "C"` so Lua can resolve them by name.

/// Flat struct Lua reads after calling frogma_peer_view.
/// Layout is fixed and explicit; Lua ffi casts a byte buffer.
///
/// Size: 48 bytes. Padding is explicit so there's no doubt.
#[repr(C)]
pub struct FrogmaPeerView {
    pub peer_id: u64,    // offset 0,  8 bytes
    pub seq: u32,        // offset 8,  4 bytes
    pub _pad0: u32,      // offset 12, 4 bytes (align t_send_ms)
    pub t_send_ms: u64,  // offset 16, 8 bytes
    pub pos_x: f32,      // offset 24
    pub pos_y: f32,      // offset 28
    pub pos_z: f32,      // offset 32
    pub yaw: f32,        // offset 36
    pub hp: u16,         // offset 40
    pub hp_max: u16,     // offset 42
    pub vocation: u8,    // offset 44
    pub pose: u8,        // offset 45
    pub _pad1: u16,      // offset 46, 2 bytes (total 48)
}

/// Push the local player's current state. Called each frame by Lua
/// after reading DD2's IL2CPP surface. Cheap: one mutex acquisition.
#[no_mangle]
pub extern "C" fn frogma_push_local_state(
    pos_x: f32,
    pos_y: f32,
    pos_z: f32,
    yaw: f32,
    hp: u16,
    hp_max: u16,
    vocation: u8,
    pose: u8,
) {
    let local = state().lock().unwrap().local_state.clone();
    *local.lock().unwrap() = LocalState {
        pos: [pos_x, pos_y, pos_z],
        yaw,
        hp,
        hp_max,
        vocation,
        pose,
    };
}

/// Number of remote peers currently in the peer table.
#[no_mangle]
pub extern "C" fn frogma_peer_count() -> usize {
    match &state().lock().unwrap().peer_table {
        Some(t) => t.len(),
        None => 0,
    }
}

/// Copy the `idx`-th peer's latest snapshot into `out`. Returns
/// `true` on success, `false` if `idx` is out of bounds, `out` is
/// null, or no peer table is initialised.
///
/// The peer order is not stable between calls — it's a hash map
/// enumeration. Lua should iterate 0..frogma_peer_count() and match
/// by peer_id if persistent identity matters.
///
/// # Safety
/// `out` must point to a writable `FrogmaPeerView`.
#[no_mangle]
pub unsafe extern "C" fn frogma_peer_view(idx: usize, out: *mut FrogmaPeerView) -> bool {
    if out.is_null() {
        return false;
    }
    let guard = state().lock().unwrap();
    let table = match &guard.peer_table {
        Some(t) => t,
        None => return false,
    };
    let snaps = table.snapshot();
    let Some((_id, s)) = snaps.get(idx) else {
        return false;
    };
    *out = FrogmaPeerView {
        peer_id: s.peer_id,
        seq: s.seq,
        _pad0: 0,
        t_send_ms: s.t_send_ms,
        pos_x: s.pos[0],
        pos_y: s.pos[1],
        pos_z: s.pos[2],
        yaw: s.yaw,
        hp: s.hp,
        hp_max: s.hp_max,
        vocation: s.vocation,
        pose: s.pose,
        _pad1: 0,
    };
    true
}

/// Our own peer_id. Useful for Lua-side self-identification in logs.
#[no_mangle]
pub extern "C" fn frogma_local_peer_id() -> u64 {
    state().lock().unwrap().peer_id
}

// Safety: REFramework owns the pointees for the plugin's lifetime,
// and we only ever read through them.
unsafe impl Send for REFrameworkPluginInitializeParam {}
unsafe impl Sync for REFrameworkPluginInitializeParam {}
unsafe impl Send for REFrameworkPluginFunctions {}
unsafe impl Sync for REFrameworkPluginFunctions {}
unsafe impl Send for REFrameworkRendererData {}
unsafe impl Sync for REFrameworkRendererData {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_view_layout() {
        assert_eq!(std::mem::size_of::<FrogmaPeerView>(), 48);
        assert_eq!(std::mem::align_of::<FrogmaPeerView>(), 8);
        // Spot-check offsets (stable C ABI).
        let v = FrogmaPeerView {
            peer_id: 0,
            seq: 0,
            _pad0: 0,
            t_send_ms: 0,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            yaw: 0.0,
            hp: 0,
            hp_max: 0,
            vocation: 0,
            pose: 0,
            _pad1: 0,
        };
        let base = &v as *const _ as usize;
        assert_eq!(&v.peer_id as *const _ as usize - base, 0);
        assert_eq!(&v.seq as *const _ as usize - base, 8);
        assert_eq!(&v.t_send_ms as *const _ as usize - base, 16);
        assert_eq!(&v.pos_x as *const _ as usize - base, 24);
        assert_eq!(&v.yaw as *const _ as usize - base, 36);
        assert_eq!(&v.hp as *const _ as usize - base, 40);
        assert_eq!(&v.pose as *const _ as usize - base, 45);
    }

    #[test]
    fn push_local_state_updates_shared() {
        // Peek at the shared state through the exported C function.
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
        // Without a running peer loop, count is 0.
        // This test may run after push_local_state_updates_shared in
        // the same process, but peer_table is still None unless
        // reframework_plugin_initialize was called.
        let n = frogma_peer_count();
        assert!(n == 0);
    }

    #[test]
    fn peer_view_rejects_null_out() {
        unsafe {
            assert!(!frogma_peer_view(0, std::ptr::null_mut()));
        }
    }
}
