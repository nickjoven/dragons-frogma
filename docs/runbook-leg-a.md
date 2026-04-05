# Runbook — Leg A: `frogma_plugin.dll` loads in DD2

**Goal:** produce `frogma_plugin.dll`, drop it in a DD2 REFramework
install, confirm REFramework logs plugin init + the peer rx/tx
threads stay alive through a title→pause cycle.

**Tracks:** Q-0004 in `knowledge/graph/seed.jsonl`.

---

## 1. Cross-compile to Windows (from WSL2 / Linux)

Use `cargo-xwin` — it pulls MSVC headers + libs from the official
Microsoft CAB packages and drives `lld-link` for a pure Linux
cross-compile. No wine, no VM.

### One-time toolchain setup

    rustup target add x86_64-pc-windows-msvc
    cargo install cargo-xwin

No `apt install lld clang` needed — `cargo-xwin` bundles clang-cl and
drives rustc's built-in `lld-link`.

### Build the plugin

You'll need to accept the MSVC EULA the first time (sets up the
~600 MB CRT/WinSDK cache under `~/.cache/cargo-xwin/`). Either pass
`XWIN_ACCEPT_LICENSE=1` or answer the prompt:

    XWIN_ACCEPT_LICENSE=1 cargo xwin build --release -p frogma-plugin \
        --target x86_64-pc-windows-msvc

Output: `target/x86_64-pc-windows-msvc/release/frogma_plugin.dll`
(~187 KB, PE32+ x86-64).

Verify exports from the Linux host:

    strings target/x86_64-pc-windows-msvc/release/frogma_plugin.dll \
        | grep -E '^frogma_|^reframework_' | sort -u

Should list: `frogma_local_peer_id`, `frogma_peer_count`,
`frogma_peer_view`, `frogma_push_local_state`,
`reframework_plugin_initialize`, `reframework_plugin_required_version`.

### Alternative: native Windows build

If you prefer building on a Windows machine with Visual Studio's
Build Tools installed:

    rustup target add x86_64-pc-windows-msvc
    cargo build --release -p frogma-plugin --target x86_64-pc-windows-msvc

Same output path.

---

## 2. Deploy to DD2

    <Steam>/steamapps/common/Dragon's Dogma 2/
      dinput8.dll                      # REFramework loader, already installed
      reframework/
        plugins/
          frogma_plugin.dll            # <- drop here
        autorun/
          frogma.lua                   # <- companion Lua script
        logs/
          re2_framework_log.txt        # <- watch this after launch

REFramework must already be installed. If not, get it from
https://github.com/praydog/REFramework/releases (pick the DD2 build).

---

## 3. Smoke-test checklist

Launch DD2. Open `reframework/logs/re2_framework_log.txt` and grep
for these lines in order:

- [ ] `Loading plugin: frogma_plugin.dll`
- [ ] `frogma_plugin.dll: reframework_plugin_required_version returned true`
- [ ] `frogma_plugin.dll: reframework_plugin_initialize returned true`

From the Lua side (REFramework's ScriptRunner console, `Insert` key
by default):

- [ ] `frogma.lua` loaded without errors
- [ ] `frogma_peer_count()` is callable and returns `0` (no peers yet)
- [ ] No Lua errors for 30 seconds of idle title screen

Then press start → pause menu → resume → pause → resume a few times:

- [ ] No crash
- [ ] No new errors in the log
- [ ] `frogma_peer_count` still callable

If the peer rx/tx threads survive that, Leg A is killed.

---

## 4. Known failure modes & first-pass diagnosis

| Symptom | Likely cause | First probe |
|---------|--------------|-------------|
| `Failed to load plugin: frogma_plugin.dll` | wrong arch (x86 vs x64), wrong CRT, missing dep DLL | `dumpbin /dependents frogma_plugin.dll` on Windows |
| `reframework_plugin_required_version returned false` | our version struct is misaligned | check `REFrameworkPluginVersion` field order matches REFramework's `include/reframework/API.h` |
| Plugin loads but crashes on DD2 init | peer thread panicking in `start()` (bind failure, etc.) | wrap `frogma_peer::start` call site with a log-on-error branch before the `return false` |
| Lua can't find `frogma_push_local_state` | DLL export name mangled | `dumpbin /exports frogma_plugin.dll` and check for `#[no_mangle]` + `extern "C"` on every export |
| Game freezes when plugin loaded | we accidentally blocked the calling thread in `reframework_plugin_initialize` | ensure `frogma_peer::start` is non-blocking (it spawns threads and returns) |

---

## 5. Exit criteria → Q-0004 closure

When all checkboxes in §3 are green, update
`knowledge/graph/seed.jsonl`:

- Flip `Q-0004.status` from `"open"` to `"answered"`
- Add a new finding `f.plugin-dll-loads-in-dd2` with:
  - `parents: ["Q-0004"]`
  - `confidence: "verified"`
  - `evidence` = the relevant log lines
  - `accessed_at` = today

Then `make seed` to regenerate `cids.lock` and commit.

This unblocks Leg B (Lua ↔ plugin bridge for real peer data) and
Legs C/D (IL2CPP camera reads + draw-hook spike), which can now
proceed against a live DD2.
