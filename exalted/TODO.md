active:

retest with the sample character
cargo run -- render assets/sample-character.toml
cargo run -- render --format markdown assets/sample-character.toml
cargo run -- render --format pdf assets/sample-character.toml -o /tmp/sample-character.pdf

markdown things essence dots are out of 10, but pdf has it out of 6

things that are wrong in the pdf renderer:
- backgrounds have dots, but no text
- charms aren't filled in at all



todo:

why does running all tests take a while?


make sure we handle favored abilities correctly
e.g. Edd telling Daz that, "you have one more Favored ability to check off, since you only picked 4 out of 5."


tell me a story using a bit of lore from the book


redo the char gen for my actual character


I want to introduce a tui that displays all fields in a character sheet. I'm not sure of the exact layout I need, and the full sheet is fairly complicated, so I'm willing to have this be fairly rough initially and we'll refine.


we left out per-scene or ephemeral combat stuff like:
tick clock, onslaught counters, anima level, DV penalty stack.


sorcery/spellcasting mechanics, rules


limit/virtue/flaw mechanics


mass combat rules


social combat rules


mortal/heroic-mortal variants


embed document-search into this app, pre-chewed rule books
