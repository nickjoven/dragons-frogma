-- frogma.lua — REFramework companion to frogma_plugin.dll
--
-- Deploy alongside the plugin:
--     <DD2>/reframework/plugins/frogma_plugin.dll
--     <DD2>/reframework/autorun/frogma.lua   <- this file
--
-- The plugin registers four Lua globals via on_lua_state_created:
--   frogma_push_local_state(x, y, z, yaw, hp, hp_max, vocation, pose)
--   frogma_peer_count() -> number
--   frogma_peer_view(idx) -> peer_id, pos_x, pos_y, pos_z, yaw, hp,
--                            hp_max, vocation, pose   (or nil)
--   frogma_local_peer_id() -> number
--
-- No ffi required — the plugin resolves Lua C API symbols at runtime
-- and pushes these as globals before autorun scripts execute.

-- Guard: if plugin didn't load (version mismatch, crash, etc.), the
-- globals won't exist.
if type(frogma_peer_count) ~= "function" then
    log.error("[frogma] plugin globals missing — is frogma_plugin.dll loaded?")
    return
end

local peer_id = frogma_local_peer_id()
log.info(string.format("[frogma] lua bound — local peer_id=%.0f", peer_id))

-- ----------------------------------------------------------------------
-- Leg C: DD2 IL2CPP surface for local player position.
-- Path per finding f.dd2-player-pos-path-mar2026 (MyDD2Mod Feb-Mar 2026).
-- ----------------------------------------------------------------------

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

    frogma_push_local_state(
        cached.x, cached.y, cached.z,
        cached.yaw,
        1, 1,             -- hp, hp_max: Leg C+1 reads these from DD2
        0, 0              -- vocation, pose: Leg C+2
    )

    local now = os.clock()
    if now - last_log >= 1.0 then
        last_log = now
        local count = frogma_peer_count()
        local staleness = cached.stale and " (stale)" or ""
        log.info(string.format(
            "[frogma] peers=%d  self=(%.2f,%.2f,%.2f) yaw=%.2f%s",
            count, cached.x, cached.y, cached.z, cached.yaw, staleness))

        -- Dump each peer once/sec for visibility during the spike.
        for i = 0, count - 1 do
            local pid, px, py, pz, pyaw = frogma_peer_view(i)
            if pid then
                log.info(string.format(
                    "[frogma]   peer[%d]=%.0f pos=(%.2f,%.2f,%.2f) yaw=%.2f",
                    i, pid, px, py, pz, pyaw))
            end
        end
    end
end)
