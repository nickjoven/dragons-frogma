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

-- ----------------------------------------------------------------------
-- Leg C: DD2 IL2CPP surface for local player position.
-- Path per finding f.dd2-player-pos-path-mar2026 (MyDD2Mod Feb-Mar 2026).
-- ----------------------------------------------------------------------

-- Surface check at load time. If types/methods have shifted, we log
-- loudly once and fall back to zero-push so the plugin still runs.
local function surface_check()
    local td = sdk.find_type_definition("app.CharacterManager")
    if not td then return false, "app.CharacterManager type missing" end
    if not td:get_method("get_ManualPlayer") then
        return false, "get_ManualPlayer method missing"
    end
    local cm = sdk.get_managed_singleton("app.CharacterManager")
    if not cm then return false, "CharacterManager singleton nil" end
    if not sdk.get_primary_camera() then return false, "primary camera nil" end
    return true
end

local surface_ok, surface_err = surface_check()
if not surface_ok then
    log.warn("[frogma] surface_check failed: " .. tostring(surface_err)
        .. " — will push zeros")
else
    log.info("[frogma] surface_check ok — reading app.CharacterManager")
end

-- Cached last-good. ManualPlayer goes nil during title/cutscene/
-- loading/warp; we hold the last successful position so the tx
-- thread keeps broadcasting something plausible.
local cached = { x = 0.0, y = 0.0, z = 0.0, yaw = 0.0, stale = true }

-- Three-tier resolve, MyDD2Mod pattern. First non-nil wins.
local function resolve_player_pose()
    local cm = sdk.get_managed_singleton("app.CharacterManager")
    if not cm then return nil end

    -- Tier 1 + 2: backing-field read, then method call.
    local mp = cm:get_field("<ManualPlayer>k__BackingField")
    if not mp then
        local ok, v = pcall(function() return cm:get_ManualPlayer() end)
        if ok then mp = v end
    end
    if not mp then return nil end

    local go = mp:get_GameObject()
    if not go then return nil end
    local tf = go:get_Transform()
    if not tf then return nil end

    -- get_UniversalPosition is f64 world-space; we downcast to f32
    -- (v0.1 precision debt, see f.universal-position-is-f64).
    local ok_pos, pos = pcall(function() return tf:call("get_UniversalPosition") end)
    if not ok_pos or not pos then
        -- Tier 3: last-ground fallback.
        local ok_lg, lg = pcall(function() return mp:call("get_LastGroundPosition") end)
        if not ok_lg or not lg then return nil end
        pos = lg
    end

    local rot = tf:get_Rotation()
    local yaw = 0.0
    if rot then
        -- Quaternion to yaw (Y-axis rotation, RE Engine Y-up).
        -- yaw = atan2(2(wy + xz), 1 - 2(y^2 + z^2))
        local x, y, z, w = rot.x, rot.y, rot.z, rot.w
        yaw = math.atan(2 * (w * y + x * z), 1 - 2 * (y * y + z * z))
    end

    return {
        x = tonumber(pos.x) or 0.0,
        y = tonumber(pos.y) or 0.0,
        z = tonumber(pos.z) or 0.0,
        yaw = yaw,
    }
end

-- Heartbeat timing.
local last_log = os.clock()
local view = ffi.new("FrogmaPeerView")

re.on_frame(function()
    if surface_ok then
        local pose = resolve_player_pose()
        if pose then
            cached.x, cached.y, cached.z, cached.yaw =
                pose.x, pose.y, pose.z, pose.yaw
            cached.stale = false
        else
            cached.stale = true
        end
    end

    lib.frogma_push_local_state(
        cached.x, cached.y, cached.z,
        cached.yaw,
        1, 1,             -- hp, hp_max: Leg C+1 reads these from DD2
        0, 0              -- vocation, pose: Leg C+2
    )

    local now = os.clock()
    if now - last_log >= 1.0 then
        last_log = now
        local count = tonumber(lib.frogma_peer_count())
        local staleness = cached.stale and " (stale)" or ""
        log.info(string.format(
            "[frogma] peers=%d  self=(%.2f,%.2f,%.2f) yaw=%.2f%s",
            count, cached.x, cached.y, cached.z, cached.yaw, staleness))

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
