--[[
	frigate behavior
-- ]]

local function is_enemy(faction)
	local our_faction = sensors.faction()
	print("faction: ", faction, our_faction)
	return faction ~= our_faction and faction ~= Faction.Unknown and faction ~= Faction.Unaligned
end

function on_update()
	if missiles:can_fire() then
		print("frigate can fire", entity)
		local contacts = sensors:contacts()
		for i, contact in ipairs(contacts) do
			print("contact ", i, contact.kind, contact.pos, contact.vel)
			if is_enemy(contact.faction) then
				print("firing at enemy...")
				missiles:fire(contact.entity)
				return
			end
			print("not enemy :(")
		end
	end
end
