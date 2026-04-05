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
use std::sync::{Mutex, OnceLock};

use frogma_peer::{PeerConfig, PeerHandle};

// ---------- REFramework ABI ---------------------------------------------
//
// Verbatim from praydog/REFramework `include/reframework/API.h` at
// plugin version 1.15.0. We capture struct sizes + field order so
// REFramework's pointers land where we expect them. Any fields we
// don't call are typed `*const c_void` — still correctly sized, but
// uninterpreted.

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

/// Subset of the REFramework plugin-functions table we care about.
/// Field order must match `REFrameworkPluginFunctions` in API.h.
/// Fields beyond what we use are kept typed as raw pointers so the
/// struct stays the right size if Rust is asked to deref past them.
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
    /// `void log_error(const char* format, ...)` — we pass fully
    /// pre-formatted strings, no varargs, so a zero-varargs extern
    /// "C" fn pointer is ABI-compatible on x86_64.
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
    /// `const REFrameworkSDKData*` — kept opaque for v0.
    pub sdk: *const c_void,
}

// ---------- Plugin state ------------------------------------------------

struct PluginState {
    peer: Option<PeerHandle>,
    functions: Option<&'static REFrameworkPluginFunctions>,
}

static STATE: OnceLock<Mutex<PluginState>> = OnceLock::new();

fn state() -> &'static Mutex<PluginState> {
    STATE.get_or_init(|| {
        Mutex::new(PluginState {
            peer: None,
            functions: None,
        })
    })
}

// ---------- REFramework entry points ------------------------------------

/// REFramework calls this first, expecting us to fill in the minimum
/// plugin API version we were built against. REFramework refuses to
/// load us if the major version doesn't match what it exposes.
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

/// Called once after REFramework loads us. We stash the function
/// table for later log calls, then spin up the peer transport with
/// the dev-default bind address and an empty peer list.
///
/// # Safety
/// REFramework guarantees `param` is a valid read-only pointer for
/// the duration of this call and the pointees live for the lifetime
/// of the plugin. We store only pointers; lifetimes are effectively
/// `'static` from our perspective.
#[no_mangle]
pub unsafe extern "C" fn reframework_plugin_initialize(
    param: *const REFrameworkPluginInitializeParam,
) -> bool {
    if param.is_null() {
        return false;
    }
    let p = &*param;

    // Stash functions table for logging + later hook registration.
    let functions: Option<&'static REFrameworkPluginFunctions> = if p.functions.is_null() {
        None
    } else {
        Some(&*p.functions)
    };

    if let Some(f) = functions {
        if let Some(log_info) = f.log_info {
            log_info(b"[frogma] plugin_initialize: starting peer loop\0".as_ptr() as *const c_char);
        }
    }

    let cfg = PeerConfig {
        peer_id: fresh_peer_id(),
        bind: "0.0.0.0:45100".parse().unwrap(),
        peers: vec![], // populated from Lua at runtime (Leg B)
        tick: std::time::Duration::from_millis(100),
    };

    // Stub provider — returns zeros. Lua will replace this via the
    // frogma_push_local_state bridge (Leg B / Task 7).
    let provider: frogma_peer::StateProvider = Box::new(|| frogma_peer::LocalState {
        pos: [0.0, 0.0, 0.0],
        yaw: 0.0,
        hp: 1,
        hp_max: 1,
        vocation: 0,
        pose: 0,
    });

    let peer = match frogma_peer::start(cfg, provider) {
        Ok(h) => h,
        Err(_) => {
            if let Some(f) = functions {
                if let Some(log_error) = f.log_error {
                    log_error(
                        b"[frogma] plugin_initialize: frogma_peer::start failed\0".as_ptr()
                            as *const c_char,
                    );
                }
            }
            return false;
        }
    };

    let mut guard = state().lock().unwrap();
    guard.peer = Some(peer);
    guard.functions = functions;

    if let Some(f) = functions {
        if let Some(log_info) = f.log_info {
            log_info(
                b"[frogma] plugin_initialize: peer loop up on 0.0.0.0:45100\0".as_ptr()
                    as *const c_char,
            );
        }
    }

    true
}

fn fresh_peer_id() -> u64 {
    // Not cryptographic. Random-ish startup id per ADR-0002.
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdead_beef)
}

// Safety: REFramework calls into our entry points on a single thread
// (the plugin-load path). The peer threads we spawn are independent
// and guard their own state. The pointers we stash point into tables
// REFramework owns for the plugin's lifetime per its contract, and
// we only ever read through them.
unsafe impl Send for REFrameworkPluginInitializeParam {}
unsafe impl Sync for REFrameworkPluginInitializeParam {}
unsafe impl Send for REFrameworkPluginFunctions {}
unsafe impl Sync for REFrameworkPluginFunctions {}
unsafe impl Send for REFrameworkRendererData {}
unsafe impl Sync for REFrameworkRendererData {}
