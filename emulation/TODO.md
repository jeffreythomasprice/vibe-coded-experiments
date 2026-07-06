running pokemon at 5x I get an actual speed of about 3.7
running at full speed I get an actual speed of about 0.84
what gives?
logging at trace maybe?


the Pacer in crates/emulator/src/pace.rs, should this be in common?
for that matter, crates/emulator/src/app.rs has a bunch of stuff related to whether we're showing the menu and pause gating and other similar things, should these be moved somewhere common so they can be used with other future frontends too?


quicksave/load, should wire into the existing save system, there should be an enum for explicit save slots in addition to the normal battery save mode
including key bindings


make all the test roms pass
needs something about exact cycle CPU emulation


link-cable, expand the serial port stub


settings and UI for stuff like:
- video device
- audio device
- resolution
- window mode / fullscreen
