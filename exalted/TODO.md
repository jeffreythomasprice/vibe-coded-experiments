I want to bulid a tool that works with exalted character sheets. We need to track everything about a character.

Some examples:
- choices made from game mechanics (e.g. which charms they have selected)
- how many dots in various attributes and virtues and abilities
- how many dots in things they have selected at character generation time, how many bonus points they spent to get that thing, or how many xp they had to spend to get that thing
- extra text for various things, e.g. name, description, background info, etc.

I want to be able to track numbers and rules for various things. Some examples:
- add up all the points spent and prove that this is a "valid" character at chargen time, or how much xp was spent on top of that after some play
- figure out how many dice I get for various actions

Our initial goal is a data structure that can hold all information about a character, that can:
- be serialized and deserialized to json
- be validated against the rules (e.g. too many dots picked at chargen, or spent more xp than earned)
- be used to generate answers to questions like: how many dice do I get when evaluating an attribute / ability check, or for the various combat rules

For now we just need the ability to work with character sheets plus unit tests. We'll add a cli commands or a UI later.



I want to introduce a cli system for working with character sheets. Some options:
- cli command to turn the json representation of the character sheet into a markdown document
- cli command that "validates" a character:
	- do they have the right number of things at character creation
	- did they spend less than or equal to their total xp for extra things



tui