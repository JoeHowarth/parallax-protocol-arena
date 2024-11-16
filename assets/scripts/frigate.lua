--[[
	frigate behavior
-- ]]
function on_update()
	if missiles:can_fire() then
		print("frigate can fire", entity)
		local contacts = sensors:contacts()
		for i, contact in ipairs(contacts) do
			print("contact ", i, contact.kind, contact.pos, contact.vel)
			if contact.kind == CraftKind.PlasmaDrone then
				print("firing...")
				missiles:fire(contact.entity)
				return
			end
		end
	end
end
