in flight:

I want to expand rules/spells.toml and rules/charms.toml to include references to the source book. They both come from the document-search tool.

We should use the title of the book instead of the path.

/home/jeff/scratch/games/source_material/free_or_stolen/Exalted 2E/Exalted 2E.pdf => Exalted 2E
/home/jeff/scratch/games/source_material/free_or_stolen/Exalted 2E/Books of Sorcery Vol. 2 - White and Black Treatises.pdf => The Books of Sorcery, Vol II

Iterate over all items in each file, find  that spell or charm in whichever source book defines it, and add a pair of properties to each item:
- source
- pages

e.g.
source = "Exalted 2E"
pages = "201-202"

Do segments of each file in parallel in subagents to speed up the process and prevent context overflow.




we just did charms.toml and spells.toml
need a new prompt for:

we left out the full charm catalog, need to go over the rulebook and extract all of them
this should be a data file we load, currently the tiny partial catalog is hard-coded

spell catalog
sorcery/spellcasting mechanics, rules




todo:

our character file format should be toml


redo the char gen for my actual character


I want to introduce a tui that displays all fields in a character sheet. I'm not sure of the exact layout I need, and the full sheet is fairly complicated, so I'm willing to have this be fairly rough initially and we'll refine.



we left out per-scene or ephemeral combat stuff like:
tick clock, onslaught counters, anima level, DV penalty stack.


limit/virtue/flaw mechanics


mass combat


social combat


mortal/heroic-mortal variants


embed document-search into this app, pre-chewed rule books
