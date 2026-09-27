in-flight:


todo:

new change:
- bug fixes:
   - "Exalted 2nd Edition Battle Wheel" now can't hide other UI
- user experience:
   - can edit names of combatants
   - tooltips for "Move" and "Dash" are more helpful


changelog should be human-only, CLAUDE.md


rustfmt, max line length 140, go format the whole project again


there should be a UI for executing extra actions in one tick
e.g. we had a scenario where GM fiat gave a player 4 actions in one tick, and the player just moved around the wheel to the position equivalent to having done the slowest action
we should expose some kind of escape hatch in the UI for indicating a player has done multiple things at once on a tick
this should expose a control for indicating what tick they next activate on, i.e. the "effective speed" of this multi-action


we should be able to keep track of health levels and defensive values too
these are independent of the actions being taken, just another part of the UI to help track combat


some actions should have no default speed, the only such example so far is "Attack"
for these actions we should require the user to put a speed in the text box before allowing them to declare this


we should make it easier (or possible?) to add new combatants in the middle of combat
there is a "Join Battle (In Progress)" action, but how does that work?


we should remember the last action for a combatant, including the speed, if provided other than default
e.g. the last action was "Attack", with speed = 4, both the dropdown and the speed text box should be pre filled for that combatant next time they're up
if the last item was a named action that was saved we should default to that including the name


"Activate Charm" is typically a 2 phase action, but this is somewhat awkward to indicate
we should make it more obvious to indicate that we've done phase 1 or phase 2 in the UI
we should make sure to update teaching tooltips to cite rules about how this works on any new parts of the UI
my goal is that the event log indicates stuff like "Activate Charm - Preparing" and "Activate Charm - Casting"


when declaring markers over spans of time, we should be able to indicate that the start of a marker is in the past
e.g. I forgot to start something 2 ticks ago, and it would have spanned 15 ticks
so instead of indicating a 13 tick duration I can actually say that it started 2 ticks ago, or on tick X, and lasts for 15 turns


show other people's cursors
but how to do this when the windows can be different sizes?


I'd like to review all the teaching tooltips for accuracy. Look them up in the book by the reference pages, check for accuracy, and verify that they make sense for that wigdet. Split off as many subagents as necessary.


dice roller
