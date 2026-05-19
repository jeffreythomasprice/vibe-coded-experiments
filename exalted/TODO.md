turn rules/charms.toml and rules/spells.toml into databases that we can look up in code
this includes a rust type that they deserialize into, with some minimal validation
all database entries must have unique ids
remove any hard-coded database of spells or charms that we were using, if any
all spells and charms that a character uses should be one of two forms:
- a reference to a database entry by id, optionally with an extra bit of descriptive text
- a complete one off custom that follows the same schema as the database entry


sorcery/spellcasting mechanics, rules


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
