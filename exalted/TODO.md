Evaluate this code against @rules/character_creation.md and the kinds of things the text of the character sheet pdf mentions in @assets/character-sheet/voidstate-fillin-ex2-solar-v1.8.txt. What's missing?





I want to introduce a cli system for working with character sheets. Some options:
- cli command to turn the json representation of the character sheet into a markdown document
- cli command that "validates" a character:
	- do they have the right number of things at character creation
	- did they spend less than or equal to their total xp for extra things


redo the char gen for my actual character


I want to introduce a tui that displays all fields in a character sheet. I'm not sure of the exact layout I need, and the full sheet is fairly complicated, so I'm willing to have this be fairly rough initially and we'll refine.



we left out per-scene or ephemeral combat stuff like:
tick clock, onslaught counters, anima level, DV penalty stack.

we left out the full charm catalog, need to go over the rulebook and extract all of them
this should be a data file we load, currently the tiny partial catalog is hard-coded

spell catalog
sorcery/spellcasting mechanics, rules

limit/virtue/flaw mechanics

mass combat

social combat

mortal/heroic-mortal variants
