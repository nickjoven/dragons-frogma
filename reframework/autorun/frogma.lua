-- frogma.lua — REFramework companion to frogma_plugin.dll
--
-- Deploy alongside the plugin:
--     <DD2>/reframework/plugins/frogma_plugin.dll
--     <DD2>/reframework/autorun/frogma.lua   <- this file
--
-- Responsibilities (MVP scaffold):
--   1. Resolve the plugin DLL via LuaJIT FFI.
--   2. Each frame, push the local player's state into the plugin
--      so the UDP tx thread has something to broadcast.
--   3. Once per second, log peer_count + our own peer_id.
--
-- This is a scaffold. IL2CPP reads for the real player position live
-- in Leg C; until then we push zeros so the pipe is exercised end to
-- end without depending on the DD2 surface being mapped.
--
-- Known limitation: REFramework's Lua is LuaJIT, so `ffi` is
-- available in most builds. If `ffi` is absent in your build, this
-- script logs once and exits — the plugin DLL still runs the UDP
-- loop, it's just invisible from Lua until Leg B wires the proper
-- on_lua_state_created binding path.

local ok_ffi, ffi = pcall(require, "ffi")
if not ok_ffi then
    log.warn("[frogma] ffi unavailable — plugin runs headless")
    return
end

-- C signatures must match frogma-plugin/src/lib.rs exports.
ffi.cdef [[
    typedef struct {
        uint64_t peer_id;
        uint32_t seq;
        uint32_t _pad0;
        uint64_t t_send_ms;
        float    pos_x;
        float    pos_y;
        float    pos_z;
        float    yaw;
        uint16_t hp;
        uint16_t hp_max;
        uint8_t  vocation;
        uint8_t  pose;
        uint16_t _pad1;
    } FrogmaPeerView;

    void     frogma_push_local_state(
                float x, float y, float z, float yaw,
                uint16_t hp, uint16_t hp_max,
                uint8_t vocation, uint8_t pose);
    size_t   frogma_peer_count(void);
    bool     frogma_peer_view(size_t idx, FrogmaPeerView* out);
    uint64_t frogma_local_peer_id(void);
]]

-- The plugin DLL is already loaded by REFramework, so its symbols
-- live in the host process. Passing nil / empty name to ffi.load
-- resolves against the current process on Windows.
local ok_lib, lib = pcall(function()
    -- Try by module name first (REFramework's DLL list), then fall
    -- back to the host process symbol table.
    local candidates = { "frogma_plugin", "" }
    for _, name in ipairs(candidates) do
        local ok, l = pcall(ffi.load, name)
        if ok then return l end
    end
    error("could not resolve frogma_plugin exports")
end)

if not ok_lib then
    log.error("[frogma] " .. tostring(lib))
    return
end

local peer_id = lib.frogma_local_peer_id()
log.info(string.format("[frogma] lua bound to plugin — local peer_id=%s",
    tostring(peer_id)))

-- Heartbeat timing.
local last_log = os.clock()
local view = ffi.new("FrogmaPeerView")

re.on_frame(function()
    -- Leg C will replace these zeros with real IL2CPP reads:
    --   sdk.get_managed_singleton("app.CharacterManager")
    --     :get_ManualPlayer():get_GameObject():get_Transform()
    --     :get_Position() / get_Rotation()
    lib.frogma_push_local_state(
        0.0, 0.0, 0.0,    -- x, y, z
        0.0,              -- yaw
        1, 1,             -- hp, hp_max
        0, 0              -- vocation, pose
    )

    local now = os.clock()
    if now - last_log >= 1.0 then
        last_log = now
        local count = tonumber(lib.frogma_peer_count())
        log.info(string.format("[frogma] peers=%d", count))

        -- Dump each peer once/sec for visibility during the spike.
        for i = 0, count - 1 do
            if lib.frogma_peer_view(i, view) then
                log.info(string.format(
                    "[frogma]   peer[%d]=%s pos=(%.2f,%.2f,%.2f) yaw=%.2f",
                    i, tostring(view.peer_id),
                    view.pos_x, view.pos_y, view.pos_z, view.yaw))
            end
        end
    end
end)
