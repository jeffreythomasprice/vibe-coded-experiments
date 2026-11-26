in flight:

The "speed" and "actual" overlay in the top left corner needs to turn mostly transparent 5 seconds after making any modifications to speed. The corner this is in should be configurable, default to top-left. The delay before it turns transparent should be configurable. How transparent it is should be configurable, default 60%.


todo:

"If the core ever does need optimizing, the flamegraph says start with Ppu::tick_one_dot (per-dot loop, ~half the time) and the WaveChannel tick."


the Pacer in crates/emulator/src/pace.rs, should this be in common?
for that matter, crates/emulator/src/app.rs has a bunch of stuff related to whether we're showing the menu and pause gating and other similar things, should these be moved somewhere common so they can be used with other future frontends too?


make all the test roms pass
needs something about exact cycle CPU emulation


link-cable, expand the serial port stub


settings and UI for stuff like:
- video device
- audio device
- resolution
- window mode / fullscreen
