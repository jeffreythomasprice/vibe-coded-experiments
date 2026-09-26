in flight:

I'd like to build a robust input system. I want these features:
- keyboard, mouse, gamepad
- I want to be able to refer to a particular input mode in an enum, e.g. KeyboardKey(KeyboardKey::Escape) or MouseAxis(MouseAxis::X), joypad buttons, joypad axes, mouse buttons, etc.
- I want to be able to turn these enums into human-readable descriptions, e.g. to populate a UI for configuring this later
- I want the config to be optionally serialized to config.toml, with sensible defaults; our example config should include the defaults commented out
- all the existing inputs should go through this system, with sensibly named actions in the config file


todo:

/init


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