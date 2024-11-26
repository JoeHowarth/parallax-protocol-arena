---@diagnostic disable: deprecated
--[[
	flight controller 
-- ]]

function is_enemy(faction)
	local our_faction = sensors.faction()
	print("faction: ", faction, our_faction)
	return faction ~= our_faction and faction ~= Faction.Unknown and faction ~= Faction.Unaligned
end

PI = 3.1415

function vectorAngle(x, y)
	if type(x) == "table" or type(x) == "userdata" then
		return math.atan2(x.x, x.y)
	end
	return math.atan2(x, y)
end

function vectorAngleBetween(u, v)
	local dotProduct = u.x * v.x + u.y * v.y
	local magnitudeU = math.sqrt(u.x ^ 2 + u.y ^ 2)
	local magnitudeV = math.sqrt(v.x ^ 2 + v.y ^ 2)

	if magnitudeU == 0 or magnitudeV == 0 then
		error("One of the vectors has zero magnitude.")
	end

	return math.acos(dotProduct / (magnitudeU * magnitudeV))
end

function printTable(t, indent)
	indent = indent or 1
	print("{")
	for k, v in pairs(t) do
		local prefix = string.rep("  ", indent) .. tostring(k) .. ": "
		if type(v) == "table" then
			print(prefix .. "{")
			printTable(v, indent + 1)
			print(string.rep("  ", indent) .. "}")
		else
			print(prefix .. tostring(v))
		end
	end
	print("}")
end

target_coord = Vec2.new(500, 100)

function on_update()
	local craft_state = sensors:craft_state()
	printTable(engines:engine_info())
	printTable(craft_state)
	print(target_coord)
	print(vectorAngle(craft_state.vel))

	engines.set_engine_input(1, 0)
end
