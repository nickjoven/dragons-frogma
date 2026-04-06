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

    // Lua 5.1 / LuaJIT constants.
    const LUA_GLOBALSINDEX: c_int = -10002;

    // Lua C API function-pointer types we need.
    type LuaCFunction = unsafe extern "C" fn(LuaState) -> c_int;
    type FnTonumber = unsafe extern "C" fn(LuaState, c_int) -> f64;
    type FnPushnumber = unsafe extern "C" fn(LuaState, f64);
    type FnPushboolean = unsafe extern "C" fn(LuaState, c_int);
    type FnPushcclosure = unsafe extern "C" fn(LuaState, LuaCFunction, c_int);
    type FnSetfield = unsafe extern "C" fn(LuaState, c_int, *const c_char);
    type FnPushnil = unsafe extern "C" fn(LuaState);

    struct LuaApi {
        tonumber: FnTonumber,
        pushnumber: FnPushnumber,
        pushboolean: FnPushboolean,
        pushcclosure: FnPushcclosure,
        setfield: FnSetfield,
        pushnil: FnPushnil,
    }

    static LUA_API: OnceLock<Option<LuaApi>> = OnceLock::new();

    extern "system" {
        fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    unsafe fn resolve_sym<T>(module: *mut c_void, name: &[u8]) -> Option<T> {
        let p = GetProcAddress(module, name.as_ptr() as *const c_char);
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute_copy(&p))
        }
    }

    fn resolve_lua_api() -> Option<LuaApi> {
        unsafe {
            // LuaJIT's symbols might be in lua51.dll (standalone) or in
            // the host exe (REFramework embeds LuaJIT statically). Try
            // lua51.dll first, then fall back to the main module (null).
            let candidates: &[*const c_char] = &[
                b"lua51.dll\0".as_ptr() as *const c_char,
                std::ptr::null(),
            ];
            for &name in candidates {
                let module = GetModuleHandleA(name);
                if module.is_null() {
                    continue;
                }
                let api = (|| -> Option<LuaApi> {
                    Some(LuaApi {
                        tonumber: resolve_sym(module, b"lua_tonumber\0")?,
                        pushnumber: resolve_sym(module, b"lua_pushnumber\0")?,
                        pushboolean: resolve_sym(module, b"lua_pushboolean\0")?,
                        pushcclosure: resolve_sym(module, b"lua_pushcclosure\0")?,
                        setfield: resolve_sym(module, b"lua_setfield\0")?,
                        pushnil: resolve_sym(module, b"lua_pushnil\0")?,
                    })
                })();
                if api.is_some() {
                    return api;
                }
            }
            None
        }
    }

    fn lua_api() -> Option<&'static LuaApi> {
        LUA_API.get_or_init(|| resolve_lua_api()).as_ref()
    }

    /// Helper: register a Lua C function as a global variable.
    unsafe fn set_global(api: &LuaApi, l: LuaState, name: &[u8], f: LuaCFunction) {
        (api.pushcclosure)(l, f, 0);
        (api.setfield)(l, LUA_GLOBALSINDEX, name.as_ptr() as *const c_char);
    }

    /// Called by REFramework when a new Lua state is created.
    /// We register our four bridge functions as globals.
    pub unsafe extern "C" fn register(l: LuaState) {
        let Some(api) = lua_api() else {
            log_info(b"[frogma] lua_bridge: could not resolve Lua C API\0");
            return;
        };

        set_global(api, l, b"frogma_push_local_state\0", lua_push_local_state);
        set_global(api, l, b"frogma_peer_count\0", lua_peer_count);
        set_global(api, l, b"frogma_peer_view\0", lua_peer_view);
        set_global(api, l, b"frogma_local_peer_id\0", lua_local_peer_id);

        log_info(b"[frogma] lua_bridge: registered 4 globals\0");
    }

    // -- Lua-callable wrappers -----------------------------------------------

    /// frogma_push_local_state(x, y, z, yaw, hp, hp_max, vocation, pose)
    unsafe extern "C" fn lua_push_local_state(l: LuaState) -> c_int {
        let Some(api) = lua_api() else { return 0 };
        let x = (api.tonumber)(l, 1) as f32;
        let y = (api.tonumber)(l, 2) as f32;
        let z = (api.tonumber)(l, 3) as f32;
        let yaw = (api.tonumber)(l, 4) as f32;
        let hp = (api.tonumber)(l, 5) as u16;
        let hp_max = (api.tonumber)(l, 6) as u16;
        let voc = (api.tonumber)(l, 7) as u8;
        let pose = (api.tonumber)(l, 8) as u8;
        frogma_push_local_state(x, y, z, yaw, hp, hp_max, voc, pose);
        0
    }

    /// frogma_peer_count() -> number
    unsafe extern "C" fn lua_peer_count(l: LuaState) -> c_int {
        let Some(api) = lua_api() else { return 0 };
        let n = frogma_peer_count();
        (api.pushnumber)(l, n as f64);
        1
    }

    /// frogma_peer_view(idx) -> peer_id, pos_x, pos_y, pos_z, yaw, hp, hp_max, vocation, pose
    ///                       -> nil on failure
    unsafe extern "C" fn lua_peer_view(l: LuaState) -> c_int {
        let Some(api) = lua_api() else { return 0 };
        let idx = (api.tonumber)(l, 1) as usize;
        let guard = state().lock().unwrap();
        let table = match &guard.peer_table {
            Some(t) => t,
            None => {
                (api.pushnil)(l);
                return 1;
            }
        };
        let snaps = table.snapshot();
        let Some((_id, s)) = snaps.get(idx) else {
            drop(guard);
            (api.pushnil)(l);
            return 1;
        };
        let s = *s; // copy before dropping guard
        drop(guard);
        (api.pushnumber)(l, s.peer_id as f64);
        (api.pushnumber)(l, s.pos[0] as f64);
        (api.pushnumber)(l, s.pos[1] as f64);
        (api.pushnumber)(l, s.pos[2] as f64);
        (api.pushnumber)(l, s.yaw as f64);
        (api.pushnumber)(l, s.hp as f64);
        (api.pushnumber)(l, s.hp_max as f64);
        (api.pushnumber)(l, s.vocation as f64);
        (api.pushnumber)(l, s.pose as f64);
        9
    }

    /// frogma_local_peer_id() -> number
    unsafe extern "C" fn lua_local_peer_id(l: LuaState) -> c_int {
        let Some(api) = lua_api() else { return 0 };
        let id = frogma_local_peer_id();
        (api.pushnumber)(l, id as f64);
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
