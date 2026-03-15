from my project ideas:
- AI TTRPG, have a GM and players, storyteller agent, world building agent
- haunted house game
	- llm based content generation
	- procedural or AI generated art assets
	- Legally Distinct Haunted House Game
	- llm based players and monsters
	- multiplayer

elevator pitch: "Betrayal at House on the Hill" but with all the content procedurally or llm generated

some random ideas in no particular order:
- procedurally generated top-down room tiles
- little people that wander around in them with a rimworld aesthetic
- the original game had items, events, and omens
- items and events are pretty generic and can be kept mostly as-is
- omens could be replaced with a more generic quest line system?
- lists of item and event mechanics and then randomize among them, with procedurally generated with llm flavor text
- players have stats, theoriginal game had strength, speed, intelligence, and sanity
- the original game had you make up to your speed tiles per turn, until you had an item or event or omen trigger in a new room, or some room says you get stuck
- the original game had a house with multiple levels, and the stairs or special cards teleported you

copy-paste from AI summary of the standard item cards:
Adrenaline Shot: Might/Speed bonus, one-time use.
Amulet of the Ages: Gain 1 Knowledge.
Angel Feather: Take 1 less damage from falling.
Armor: Gain 2 Might, lose 1 Speed.
Axe: Gain 2 dice on Might attacks.
Blood Dagger: Might attack, trade Might for damage.
Bottle: Gain 1 Sanity.
Candle: Gain 2 Knowledge.
Dark Dice: Reroll dice.
Dynamite: Ranged attack, 3 damage.
Healing Salve: Heal mental or physical damage.
Idol: Might bonus.
Lucky Stone: Re-roll a die.
Medical Kit: Heal physical damage.
Music Box: Gain 1 Sanity.
Pickpocket's Gloves: Steal an item card.
Puzzle Box: Gain 1 Knowledge, lose 1 Knowledge.
Rabbit's Foot: Re-roll a die.
Revolver: Ranged attack.
Sacrificial Dagger: Might attack, gain Might.
Smelling Salts: Wake up a player.
Spear: Gain 2 dice on Might attacks. 

omen cards that are also items can not be dropped or traded like normal, you're stuck with them unless something else takes them away

omen cards and certain specific locations are often important for the haunt

client-server architecture
does it make sense to require the people take their turn in order, or can everybody go at once?
what does it do if two people reveal the same tile at once? higher speed wins, resolve ties randomly? by who clicked first?

pro con of various language and library choices:
- bevy = more awkward to sync server updates to graphical components? or maybe it's fine?
- web-first, either ts or rust = have to do everything through an API because browsers? but I get canvas 2d api

lists of real game content:
https://betrayal3rdedition.fandom.com/wiki/Rooms
https://betrayal3rdedition.fandom.com/wiki/Items

poc:
- random tiles with random arrangements of doors
- local llm generating rooms, tiles, events from a theme specification
- no omen or equivalent, just people wandering around in the rooms forever collecting items
- players are just circles placed randomly in the tile they're in
- draw just the walls and doors
- can click a player or a tile and get stats on a sidebar
- show the list of players at the top, can click on the circle or the name+icon at the top
- no multiple floors for now
- client-server architecture, stage us for multiplayer in the future
- for now it's only one player, possible actions are to indicate what actions a particular player is taking, and the end turn command

GURPS rules?
all roles are 3d6
stats are 10 or 11 on average, you have to roll under your stat to win
contested roles are comparing the relative degrees of success or failure

what kinds of effect are there?
- how might they trigger?
	- roll a stat check, degrees of success determine outcome, e.g. success means you get something, failure means you lose something
	- always
	- roll completely randomly, not a stat check; e.g. 1-2 = good outcome, 3-4 = bad outcome, 5-6 = nothing happens
- results might be:
	- permanent raise or lower a stat; e.g. take damage, heal stat to full
	- gain or lose an item
	- physical or mental damage: player's choice how to distribute among their stats
	- teleport
	- lose next turn
	- forced movement
	- modify an existing item they hold
	- permanently modify this room; move the room, gain/lose a door, permanent effect
- who does it affect
	- player triggering the effect
	- all players in that room
	- all players in the game
- when does it affect them
	- now
	- for next X turns
	- in X turns

what can items do?
- permanent effects; almost always raise/lower stat; cursed items that lower stats might not be droppable or tradable
- trigger an effect on demand
	- once, or some number of times
	- on yourself, on another player, or either
	- in the same tile, or in straight lines through doors (guns), or anywhere

what can events do?
- trigger an effect

kinds of object:
- GeneratorContext = list of strings, used as context for generating other stuff; individual strings can be tagged as applying to everything, or to specific other kinds of objects
- Player = name, description, stats
- Room = a description, how many doors it has and in what arrangement (e.g. must be a straight hallway, must be a corner, etc.)
- Effects = see above
- Item = name, description, effect
- Event = name, description, effect
