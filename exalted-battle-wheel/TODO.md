in-flight:

I'd like to design a basic combat tick system. My goal is to have an interface that allows me to enter the people fighting, determine what order they start in by soliciting a minimal set of stats for each combatant, and then have them trigger actions on their turn. We should support multi-turn actions like spells. We should be able to cancel or undo such an action, e.g. something interrupts that spell before it triggers.

My goal is a pretty display for the actual turn, i.e. the "battle wheel" concept that lets me visually see how many ticks we have to turn the wheel before we get to each event. Mousing over elements on the wheel should give the full info for that event and who triggered it.


todo:

dice roller

keep track of more stuff:
  2. + Defense tracking
     Adds Dodge DV / Parry DV as entered numbers, live DV penalty from the current action, DV refresh timing, and per-attacker onslaught counters. Still no attack or damage resolution.
  3. + Attacks & health
     Full Chapter Four loop: weapons, accuracy, soak, damage, health track and wound penalties. Much larger; the weapon tables in RULES.md are OCR-damaged and would need verification
     against the books first.

undo stack visibility
event log

tooltips or tutorials on page to describe wtf all this means