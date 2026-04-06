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

    // Register Lua bindings via on_lua_state_created.
    // IMPORTANT: extract the fn pointer and drop the guard BEFORE calling
    // register(). REFramework may invoke our callback immediately, and
    // the callback calls log_info → state().lock(), which would deadlock
    // if we were still holding the outer lock.
    let on_lua_created = state()
        .lock()
        .unwrap()
        .functions
        .and_then(|f| f.on_lua_state_created);
    if let Some(register) = on_lua_created {
        register(on_lua_state_created);
        log_info(b"[frogma] registered on_lua_state_created callback\0");
    }

    let peer_id = state().lock().unwrap().peer_id;
    let cfg = PeerConfig {
        peer_id,
        bind: "0.0.0.0:45100".parse().unwrap(),
        peers: vec![],
        tick: std::time::Duration::from_millis(100),
    };

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

// ---------- Lua bridge via on_lua_state_created -------------------------
//
// REFramework's Lua sandbox strips LuaJIT `ffi`, so we can't call DLL
// exports from Lua directly. Instead we register Lua globals from the
// plugin's on_lua_state_created callback. The Lua C API symbols
// (lua_pushnumber, lua_setfield, etc.) are resolved at runtime from
// the host process — REFramework ships LuaJIT, so the symbols are
// already in memory.
//
// On Linux builds we skip this entire section (no Windows API for
// GetProcAddress, and the Lua bridge is only exercised inside DD2).

#[cfg(target_os = "windows")]
mod lua_bridge {
    use super::*;

    // Lua 5.1 C API — linked from lua51-msvc/lua51.lib (pre-built via
    // zig cc from lua-src 5.1.5). Binary-compatible with LuaJIT 2.1.
    #[repr(C)]
    pub struct LuaStateOpaque {
        _private: [u8; 0],
    }

    const LUA_GLOBALSINDEX: c_int = -10002;

    type LuaCFn = unsafe extern "C" fn(*mut LuaStateOpaque) -> c_int;

    extern "C" {
        fn lua_tonumber(l: *mut LuaStateOpaque, idx: c_int) -> f64;
        fn lua_pushnumber(l: *mut LuaStateOpaque, n: f64);
        fn lua_pushnil(l: *mut LuaStateOpaque);
        fn lua_pushcclosure(l: *mut LuaStateOpaque, f: Option<LuaCFn>, n: c_int);
        fn lua_setfield(l: *mut LuaStateOpaque, idx: c_int, k: *const c_char);
    }

    unsafe fn set_global(l: *mut LuaStateOpaque, name: &[u8], f: LuaCFn) {
        lua_pushcclosure(l, Some(f), 0);
        lua_setfield(l, LUA_GLOBALSINDEX, name.as_ptr() as *const c_char);
    }

    pub unsafe extern "C" fn register(l: LuaState) {
        let l = l as *mut LuaStateOpaque;
        set_global(l, b"frogma_push_local_state\0", w_push_local_state);
        set_global(l, b"frogma_peer_count\0", w_peer_count);
        set_global(l, b"frogma_peer_view\0", w_peer_view);
        set_global(l, b"frogma_local_peer_id\0", w_local_peer_id);
        log_info(b"[frogma] lua_bridge: registered 4 globals\0");
    }

    unsafe extern "C" fn w_push_local_state(l: *mut LuaStateOpaque) -> c_int {
        frogma_push_local_state(
            lua_tonumber(l, 1) as f32,
            lua_tonumber(l, 2) as f32,
            lua_tonumber(l, 3) as f32,
            lua_tonumber(l, 4) as f32,
            lua_tonumber(l, 5) as u16,
            lua_tonumber(l, 6) as u16,
            lua_tonumber(l, 7) as u8,
            lua_tonumber(l, 8) as u8,
        );
        0
    }

    unsafe extern "C" fn w_peer_count(l: *mut LuaStateOpaque) -> c_int {
        lua_pushnumber(l, frogma_peer_count() as f64);
        1
    }

    unsafe extern "C" fn w_peer_view(l: *mut LuaStateOpaque) -> c_int {
        let idx = lua_tonumber(l, 1) as usize;
        let guard = state().lock().unwrap();
        let table = match &guard.peer_table {
            Some(t) => t,
            None => { lua_pushnil(l); return 1; }
        };
        let snaps = table.snapshot();
        let Some((_id, s)) = snaps.get(idx) else {
            drop(guard);
            lua_pushnil(l);
            return 1;
        };
        let s = *s;
        drop(guard);
        lua_pushnumber(l, s.peer_id as f64);
        lua_pushnumber(l, s.pos[0] as f64);
        lua_pushnumber(l, s.pos[1] as f64);
        lua_pushnumber(l, s.pos[2] as f64);
        lua_pushnumber(l, s.yaw as f64);
        lua_pushnumber(l, s.hp as f64);
        lua_pushnumber(l, s.hp_max as f64);
        lua_pushnumber(l, s.vocation as f64);
        lua_pushnumber(l, s.pose as f64);
        9
    }

    unsafe extern "C" fn w_local_peer_id(l: *mut LuaStateOpaque) -> c_int {
        lua_pushnumber(l, frogma_local_peer_id() as f64);
        1
    }
}

// On non-Windows, provide a no-op callback so plugin_initialize compiles.
#[cfg(not(target_os = "windows"))]
mod lua_bridge {
    use super::*;
    pub unsafe extern "C" fn register(_l: *mut c_void) {}
}

/// The on_lua_state_created callback — dispatches to lua_bridge::register.
unsafe extern "C" fn on_lua_state_created(l: LuaState) {
    lua_bridge::register(l);
}

// ---------- C ABI bridge (still exported for direct use) ----------------

#[repr(C)]
pub struct FrogmaPeerView {
    pub peer_id: u64,
    pub seq: u32,
    pub _pad0: u32,
    pub t_send_ms: u64,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub yaw: f32,
    pub hp: u16,
    pub hp_max: u16,
    pub vocation: u8,
    pub pose: u8,
    pub _pad1: u16,
}

#[no_mangle]
pub extern "C" fn frogma_push_local_state(
    pos_x: f32, pos_y: f32, pos_z: f32, yaw: f32,
    hp: u16, hp_max: u16, vocation: u8, pose: u8,
) {
    let local = state().lock().unwrap().local_state.clone();
    *local.lock().unwrap() = LocalState {
        pos: [pos_x, pos_y, pos_z],
        yaw, hp, hp_max, vocation, pose,
    };
}

#[no_mangle]
pub extern "C" fn frogma_peer_count() -> usize {
    match &state().lock().unwrap().peer_table {
        Some(t) => t.len(),
        None => 0,
    }
}

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
        peer_id: s.peer_id, seq: s.seq, _pad0: 0,
        t_send_ms: s.t_send_ms,
        pos_x: s.pos[0], pos_y: s.pos[1], pos_z: s.pos[2],
        yaw: s.yaw, hp: s.hp, hp_max: s.hp_max,
        vocation: s.vocation, pose: s.pose, _pad1: 0,
    };
    true
}

#[no_mangle]
pub extern "C" fn frogma_local_peer_id() -> u64 {
    state().lock().unwrap().peer_id
}

// Safety: REFramework owns the pointees for the plugin's lifetime.
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
        let v = FrogmaPeerView {
            peer_id: 0, seq: 0, _pad0: 0, t_send_ms: 0,
            pos_x: 0.0, pos_y: 0.0, pos_z: 0.0, yaw: 0.0,
            hp: 0, hp_max: 0, vocation: 0, pose: 0, _pad1: 0,
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

    #[test]
    fn peer_view_rejects_null_out() {
        unsafe { assert!(!frogma_peer_view(0, std::ptr::null_mut())); }
    }
}
