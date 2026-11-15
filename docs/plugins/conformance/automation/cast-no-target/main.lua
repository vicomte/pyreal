-- In a world that allows buffs and denies combat: a monster selected, a debuff cast with no
-- target lands on the selection, so it is combat and refused.
local fight = require("dereth.combat")
on("session.login", function()
	task.spawn(function()
		print("the magic stance for a buff", fight.ready_magic({ purpose = "buff" }))
		print("the monster selected", pawait(act.select(0x80000071)))
		print("a debuff with no target", pawait(magic.cast(23)))
	end)
end)
