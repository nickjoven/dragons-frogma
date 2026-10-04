fn main() {
    // On Windows targets, link against the pre-built Lua 5.1 static library.
    // This was compiled from lua-src-550.0.0/lua-5.1.5 using zig cc targeting
    // x86_64-windows-msvc with the cargo-xwin CRT/SDK headers.
    //
    // The Lua C API in Lua 5.1 is binary-compatible with LuaJIT 2.1's C API
    // (same struct layouts, same calling conventions). REFramework statically
    // links LuaJIT but doesn't export the Lua C API symbols, so we vendor
    // our own copy. The functions operate on the lua_State* pointer passed
    // to us by REFramework's on_lua_state_created callback.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rustc-link-search=native={manifest_dir}/lua51-msvc");
        println!("cargo:rustc-link-lib=static=lua51");
    }
}
