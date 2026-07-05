make all the test roms pass
needs something about exact cycle CPU emulation


Input bindings for:
- emulation speed (native, relative, unbounded)
- quicksave/load slots
- pause/resume


I'd like to introduce a UI system that can overlay on top of the emulator video system. We're going to need the following functionality:
- free-floating informational text, e.g. "Paused" or "FPS: 60"
- buttons, with hover effects for the mouse cursor or when click-holding on the button
- items should have some basic layout functionality, e.g. easily place something in the top-middle of the screen, or the lower-right corner, or in such a place relative to some other bounding rectangle
- a text box with a border
- scrolling

We should embed 'assets/Early GameBoy.ttf' as a font resource in whichever crate makes the most sense

Kinds of UI we need:
- when not in a running ROM, a menu with these options: "Select ROM", "Options", "Exit"
- when a ROM is loaded and running, a menu with these options: "Select A Different ROM", "Options", "Exit"
- an options menu with these options: "Input Bindings", "Back"
- an input bindings menu with a scroll bar with all of the possible input bindings in some sensible order; this is a table with the name of the input binding on the left-most column, then a 2nd and 3rd column displaying a text box describing what key/mouse button/gamepad button is bound here, or empty if nothing is bound there; if more than 2 input bindings exist we can display just the first two


quicksave/load, should wire into the existing save system, there should be an enum for explicit save slots in addition to the normal battery save mode


link-cable, expand the serial port stub


settings and UI for stuff like:
- video device
- audio device
- resolution
- window mode / fullscreen
