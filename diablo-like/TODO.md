in flight:


todo:

needs a configuration UI for setting inputs
including some visual feedback for what key or axis or joypad or whatever is being pressed
like display in text what inputs are currently pressed


physics needs to have sliding, e.g. moving into an angled wall should slide instead of locking the player's circle


zoom levels
render using the proper texture mechanism with the "normal" view radius
but then render in wireframe everything outside that radius if we're zoomed out far enough


we currently render a single texture worth of rendered chunks
multiple textures? how many textures until webgl starts slowing down?
alternatively, how fast can we render new textures?
another thread / web worker to render another texture for upcoming chunks?


mobs, random, bosses


loot, random


skills, spells, skill trees
random skills?


director, plan what groups of mobs or loot shows up in what order


auto-balance test system, with a bot that simulates player actions?


font, unicode rendering, dynamic texture atlas
use some existing ttf to texture library?