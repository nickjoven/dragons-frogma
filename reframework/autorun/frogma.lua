-- frogma.lua — REFramework companion to frogma_plugin.dll
--
-- IPC via text files (no ffi, no Lua C API registration needed):
--   Lua writes  reframework/frogma_local.txt   (local player state)
--   Lua reads   reframework/frogma_peers.txt   (remote peer snapshots)
--   Plugin reads frogma_local.txt, writes frogma_peers.txt.
--
-- File format:
--   frogma_local.txt:  "x y z yaw hp hp_max vocation pose\n"
--   frogma_peers.txt:  "count\npeer_id x y z yaw hp hp_max voc pose\n..."
--   frogma_peer_id.txt: "peer_id\n"

local LOCAL_PATH  = "reframework/frogma_local.txt"
local PEERS_PATH  = "reframework/frogma_peers.txt"
local PEERID_PATH = "reframework/frogma_peer_id.txt"

-- Read our peer_id (written once by the plugin at init).
local peer_id = 0
local f = io.open(PEERID_PATH, "r")
if f then
    peer_id = tonumber(f:read("*l")) or 0
    f:close()
    log.info(string.format("[frogma] lua bound — local peer_id=%.0f (file IPC)", peer_id))
else
    log.warn("[frogma] peer_id file not found — plugin may not have loaded")
end

-- ----------------------------------------------------------------------
-- Leg C: DD2 IL2CPP surface for local player position.
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

local cached = { x = 0.0, y = 0.0, z = 0.0, yaw = 0.0, stale = true }

local function resolve_player_pose()
    local cm = sdk.get_managed_singleton("app.CharacterManager")
    if not cm then return nil end

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

    local ok_pos, pos = pcall(function() return tf:call("get_UniversalPosition") end)
    if not ok_pos or not pos then
        local ok_lg, lg = pcall(function() return mp:call("get_LastGroundPosition") end)
        if not ok_lg or not lg then return nil end
        pos = lg
    end

    local rot = tf:get_Rotation()
    local yaw = 0.0
    if rot then
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

-- Parse frogma_peers.txt into a table of peer snapshots.
local function read_peers()
    local fh = io.open(PEERS_PATH, "r")
    if not fh then return {} end
    local count_line = fh:read("*l")
    local count = tonumber(count_line) or 0
    local peers = {}
    for i = 1, count do
        local line = fh:read("*l")
        if not line then break end
        local pid, px, py, pz, pyaw, php, phpm, pvoc, ppose =
            line:match("(%S+) (%S+) (%S+) (%S+) (%S+) (%S+) (%S+) (%S+) (%S+)")
        if pid then
            peers[#peers + 1] = {
                peer_id = tonumber(pid) or 0,
                x = tonumber(px) or 0, y = tonumber(py) or 0, z = tonumber(pz) or 0,
                yaw = tonumber(pyaw) or 0,
                hp = tonumber(php) or 0, hp_max = tonumber(phpm) or 0,
                vocation = tonumber(pvoc) or 0, pose = tonumber(ppose) or 0,
            }
        end
    end
    fh:close()
    return peers
end

local last_log = os.clock()

re.on_frame(function()
    -- Resolve local player pose and write to file for the plugin.
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

    -- Write local state for the plugin's tx thread.
    local fh = io.open(LOCAL_PATH, "w")
    if fh then
        fh:write(string.format("%.6f %.6f %.6f %.6f %d %d %d %d\n",
            cached.x, cached.y, cached.z, cached.yaw,
            1, 1, 0, 0))
        fh:close()
    end

    -- Read peer snapshots from the plugin.
    local peers = read_peers()

    local now = os.clock()
    if now - last_log >= 1.0 then
        last_log = now
        local staleness = cached.stale and " (stale)" or ""
        log.info(string.format(
            "[frogma] peers=%d  self=(%.2f,%.2f,%.2f) yaw=%.2f%s",
            #peers, cached.x, cached.y, cached.z, cached.yaw, staleness))

        for i, p in ipairs(peers) do
            log.info(string.format(
                "[frogma]   peer[%d]=%.0f pos=(%.2f,%.2f,%.2f) yaw=%.2f",
                i, p.peer_id, p.x, p.y, p.z, p.yaw))
        end
    end
end)
