local MissileBot
local PlasmaBot
local Transform

local function define_components()
	if MissileBot == nil then
		MissileBot = world:get_type_by_name("MissileBot")
		print("MissileBot", MissileBot)
	end
	if PlasmaBot == nil then
		PlasmaBot = world:get_type_by_name("PlasmaBot")
		print("PlasmaBot", PlasmaBot)
	end
	if Transform == nil then
		Transform = world:get_type_by_name("Transform")
		print("Transform", Transform)
	end
end

function on_update()
	define_components()

	if missiles:can_fire() then
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

-- function on_update()
-- 	define_components()
-- 	if not has_fired then
-- 		local min = 10000000000
-- 		local min_ent = nil
-- 		local our_transform = world:get_component(entity, Transform)
-- 		local our_pos = our_transform.translation
--
-- 		for other_entity, trans in world:query(Transform):with(PlasmaBot):iter() do
-- 			local dist = our_pos:distance(trans.translation)
-- 			if dist < min then
-- 				min_ent = other_entity
-- 				min = dist
-- 			end
-- 		end
--
-- 		print("closest: ", min_ent, min)
-- 		missile:fire(entity, min_ent)
-- 		has_fired = true
-- 	end
-- end
