# Rules

Never edit TODO.md. That's for human note taking. The user will be manually updating that file when tasks are complete after reviewing.

# Crate structure

This is a Cargo workspace. Crates live under `crates/`:

- `common` — shared, system-agnostic utilities used across the workspace. Owns
  logging/tracing init (`common::init()`), the abstract persistent-state
  interface (`common::storage`), the backend-agnostic settings model
  (`common::settings`), and the shared video `ScaleMode` (`common::scale`). Its
  `build.rs` discovers every workspace member crate at build time and bakes their
  names into `WORKSPACE_CRATE_TARGETS`, so the default log filter (our crates at
  `trace`, everything else at `warn`) stays correct as crates are added — nothing
  is hardcoded. `RUST_LOG` overrides it. Keep `common` free of system-specific
  (`gameboy`) and windowing/GPU (`wgpu`) dependencies.
- `gameboy` — Game Boy (SM83) emulator core. System-agnostic emulation logic,
  no UI/windowing dependencies.
- `video` — generic wgpu video output (`PixelBufferRenderer`, pixel-format
  conversion, letterboxing) plus the wgpu **egui overlay backend**
  (`EguiRenderer`). Consumes `common::ScaleMode`; owns only the wgpu-bound
  `Background` fill color. The only crate that touches `egui-wgpu`.
- `audio` — generic cpal audio output (`AudioOutput` trait + `CpalAudioOutput`
  impl, streaming rate conversion). The audio analog of `video`: it is the only
  crate that touches the real-device audio library (cpal), so a headless build
  can swap in a different sink. Keep 3rd-party audio deps isolated here.
- `ui` — the backend-agnostic menu overlay, built on **`egui` core only** (no
  `wgpu`, `winit`, or `gameboy`). Owns the menu screens, the navigation/capture
  state machine, the input-bindings editor, and the embedded pixel font. It is the
  reuse boundary: a second frontend supplies its own input/render backends and
  calls the same `ui::draw`. See "UI overlay" below.
- `emulator` — desktop UI shell (winit + wgpu) that hosts emulator cores. Binary
  entry point; calls `common::init()` at startup. Owns the file-system
  implementation of the storage traits (`FileStore`) and the winit/wgpu wiring
  that drives the `ui`/`video`/`audio` crates.

New crates go under `crates/` and get added to `[workspace] members` in the root
`Cargo.toml`; they are then picked up by the logging filter automatically.

# Persistent state

The persistent-state system splits an **abstract interface** from its **backend**:

- `common::storage` defines the traits (`SettingsStore`, `RomLibrary`,
  `SaveStore`, and the `PersistentStore` supertrait) plus opaque identities
  (`RomId`, `SaveId`, `SaveSlot`) and a `StorageError`. It is deliberately
  **path-free**: nothing here names a file or directory, so a future non-file
  backend (database, cloud) can implement the same traits. `common::settings`
  holds only backend-agnostic settings (graphics scale mode; the generic input
  bindings). Where ROMs/saves live is *not* here — that is backend config.
- `emulator/src/storage.rs`'s `FileStore` is the only implementation today. It
  owns **all** filesystem concerns: resolving the config dir via the `directories`
  crate (`~/.config/emulator`), the on-disk `settings.toml` schema (which adds a
  `[paths]` section for the configurable `saves_dir`/`roms_dir`), every `std::fs`
  call, and sanitizing opaque ids into safe file names. `--config <path>` overrides
  the settings file location.

Loaded settings drive the `video` scale mode at startup (`Gfx::new`). Battery
saves route through `SaveStore` (`emulator/src/rom.rs`), keyed by the cartridge
header title. This trait-in-`common` / impl-in-`emulator` split is the pattern for
future backends and future systems.

# Game Boy resources

These are **specific to the `gameboy` crate**. As we add more systems, each new
system should get its own analogous set of resources (references, notes, test
ROMs) rather than piling into these.

- `references/gameboy/` — local copies of the hardware docs (Pan Docs mirror,
  SM83 instruction set, CPU manual PDF, PPU and timer wiki pages).
  `references/gameboy/README.md` lists the external source URLs.
- `GAMEBOY_IMPLEMENTATION_NOTES.md` — working notes distilled from the docs in
  `references/gameboy/`, organized by subsystem. Start here to get oriented;
  each section links back to the source doc for detail. Scope is the original
  DMG Game Boy first, with CGB behavior tagged `[CGB — future]`.
- Test ROMs (Blargg / mooneye style) live **outside this repo** at
  `/home/jeff/workspaces/personal/gb-test-roms`, cloned from
  https://github.com/jeffreythomasprice/gb-test-roms.

# Game Boy CPU core

The SM83 CPU lives in `gameboy/src/cpu/`, split by responsibility:

- `bus.rs` — the `Bus` trait (`read8`/`write8`, plus little-endian `read16`/
  `write16` defaults), the **only** seam between the CPU and memory. This mirrors
  the trait-in-`common` pattern: the real system memory map (cartridge + WRAM +
  VRAM + OAM + I/O + HRAM) will just be another `impl Bus` later. The CPU is
  generic over `B: Bus` at the *method* level (static dispatch, no vtable on the
  hot path) while `Cpu` itself stays concrete and `Clone`/`Debug` for future save
  states. `read8` takes `&mut self` on purpose, to leave room for a future
  cycle-stepped model and side-effectful I/O reads. `FlatMemory` (also here) is a
  64 KiB test double used to unit-test the CPU with no other subsystems wired up.
- `flags.rs` / `registers.rs` — the `F` register (a newtype that structurally
  enforces the always-zero low nibble, so even `POP AF` can't set it) and the
  register file (`post_boot_dmg()` seeds the skip-boot-ROM state).
- `alu.rs` — pure `(result, Flags)` primitives; all the fiddly half-carry / `DAA`
  / `ADD SP,r8` low-byte-carry rules live here and are exhaustively unit-tested in
  isolation, so the opcode tables stay thin.
- `ops.rs` — operand/control-flow helpers shared by both opcode tables (the 3-bit
  `B C D E H L (HL) A` operand code, the branch conditions, `jr`/`jp`/`call`/
  `ret_cc`/`rst`).
- `execute.rs` (base) / `cb.rs` (`$CB`) — the decode tables. The regular blocks
  (`LD r,r'`, 8-bit ALU, the whole `$CB` page, register `INC`/`DEC`/`LD r,d8`) are
  decoded *structurally* from the opcode bits; the irregular opcodes are spelled
  out to mirror the opcode matrix.
- `mod.rs` — the `Cpu` struct and `step<B: Bus>(&mut B) -> u32`, the
  **instruction-stepped** driver: it runs one whole instruction (or one interrupt
  dispatch, or one idle halted/stopped cycle) and returns the **T-states**
  consumed.

Interrupt handling lives on the CPU (`interrupts.rs`): `IME` and the whole
dispatch sequence (priority pick, clear `IME`, clear the `IF` bit, push `PC`, jump
to the vector) are here because `HALT`, the halt bug, the `EI` one-instruction
delay, and `RETI` are all defined in terms of them. Only `IME` is a CPU field —
the per-source `IE` (`0xFFFF`) and `IF` (`0xFF0F`) bits are **memory-mapped** and
reached through the `Bus`; `Cpu::request_interrupt` is the seam the PPU/timer/
joypad will use to set `IF`. Undefined opcodes lock the CPU (a `RunState::Locked`
state + a `CpuFault` surfaced via `Cpu::fault()`) rather than panicking, keeping
`step()` free of a hot-path `Result`.

The CPU is now wired to a real memory bus (see below). A top-level `Emulator`
loop that owns and drives the CPU + bus is still a later item; the `Emulator`
placeholder in `lib.rs` remains a stub.

# Game Boy cartridge & MBC

`gameboy/src/cartridge/` loads a ROM image and dispatches its memory-bank
controller. Layout:

- `mod.rs` — the `Cartridge`: it **owns** the ROM and external-RAM byte buffers
  (`rom: Vec<u8>`, `ram: Vec<u8>`) plus an `Mbc`. `from_bytes(&[u8])` parses the
  header and builds the mapper; a bad header checksum or a ROM-length/declared-size
  disagreement **warn and load anyway** (homebrew and hand-assembled test ROMs are
  more useful than strict refusal), while only a too-short image or an unknown
  ROM/RAM size code is a hard `CartridgeError`. `read`/`write` route
  `0x0000-0x7FFF` (ROM; writes are MBC *commands*, not data) and `0xA000-0xBFFF`
  (external RAM/registers) to the mapper; everything else is `0xFF` open bus. The
  gameboy crate is deliberately filesystem-free — there is no `from_file`; the
  `emulator` crate owns all IO.
- `header.rs` — full `0x0100-0x014F` header parse: title, cartridge type
  (`0x0147` → a `Mapper` enum + `has_ram/has_battery/has_timer/has_rumble/
  has_sensor` flags), ROM size (`0x0148`), RAM size (`0x0149`), CGB/SGB flags,
  checksums.
- `mbc/` — one small state machine per mapper. Each holds **only its registers**
  (current bank, enable flags, mode, RTC) and never the byte buffers, so its whole
  job is translating a CPU address to a flat ROM offset and servicing the RAM/
  register window. Dispatch is a plain `enum Mbc` + `match` (via the `dispatch!`
  macro) over the `MbcImpl` trait, chosen over `Box<dyn>` because the mapper set is
  closed and this is the hottest path in the emulator (every memory access), and an
  enum stays trivially `Debug`/`Clone` for future save states. Shared helpers
  (`is_ram_enable_value`, `simple_ram_read`/`simple_ram_write`) keep the common
  banked-RAM cases uniform.

**Real and tested:** ROM-only, MBC1 (5+2 bit banking, the `0x20/0x40/0x60`
aliasing quirk, ROM/RAM mode select), MBC2 (built-in 512×4-bit RAM, address-bit-8
register discrimination), MBC3 (7-bit ROM bank + RAM/RTC select + the RTC in
`mbc/rtc.rs`), MBC5 (9-bit ROM bank, rumble bit). **Experimental stubs** that load
with a warning: MMM01, HuC1/HuC3, MBC6, MBC7, TAMA5, Pocket Camera. `Mbc4`
(apocryphal) falls back to MBC3; an unknown type byte falls back to MBC5 banking.

The MBC3 RTC (`mbc/rtc.rs`) is a live wall clock: `base` registers plus an
absolute-time `anchor`, with the current time computed on latch by adding the
elapsed real seconds — so it advances even across a close/reopen (see battery
saves below). It is unrelated to the DMG timer/divider.

# Game Boy battery saves & RTC persistence

Carts flagged `+BATTERY` persist their `0xA000-0xBFFF` RAM (and, for MBC3+timer,
the RTC) to a save file. The split follows the trait-in-`common`/impl-in-`emulator`
pattern:

- **Core (`gameboy`):** `Cartridge::has_battery()`, and the opaque save-blob pair
  `save_bytes()` / `load_bytes()`. The blob is external RAM followed by a
  fixed-size RTC trailer (`RTC_SAVE_LEN` bytes: `base` + `latched` registers + the
  wall-clock anchor as Unix seconds) **only when** the mapper has a clock. Because
  RTC presence is a property of the mapper, `load_bytes` splits the trailer off
  deterministically at `ram.len()` (even for a timer-only cart with zero RAM); a
  blob too short to carry a trailer loads as RAM-only and leaves the clock at its
  power-on value. `save_ram`/`load_ram` remain the RAM-only primitives. Persisting
  the *absolute* anchor is what lets a restored RTC keep ticking through the time
  the emulator was closed, like real hardware.
- **Bridge (`emulator/src/rom.rs`):** `save_id_for` derives the `SaveId` from the
  cartridge header title (falling back to the ROM file stem for blank-title
  homebrew/test ROMs); `restore_battery` reads `SaveSlot::Battery` and calls
  `load_bytes`; `save_battery` writes `save_bytes()`. All IO is behind the abstract
  `SaveStore`; `FileStore` maps the slot to `{sanitized-title}.sav` under the
  configurable saves dir.
- **Triggers (`emulator/src/app.rs`):** loading a ROM restores the save in
  `run_rom` (`main.rs`) before the app starts. Two save paths exist: an **on-request**
  manual save via the generic `SaveBattery` action (default key **F5**, added to
  `common`'s `GenericAction`) — `InputRouter` queues fired generic actions and
  `App::apply_generic_actions` drains and acts on them — and an **on-exit** flush
  via `impl Drop for App`, which fires when the event loop returns and so covers
  every exit path (graceful close, surface loss, init failure), not just
  `CloseRequested`.

Caveat: there is still no top-level CPU+`SystemBus` run loop, so `App` holds a
standalone `Cartridge` whose RAM nothing mutates yet — the save/load path is
correct and fully unit-tested, but saves capture unchanging data until the run
loop lands and drives the cartridge.

# Game Boy memory bus

`gameboy/src/memory.rs`'s `SystemBus` is the real DMG memory-map implementation
of `cpu::Bus` — the second `impl Bus` in the crate, alongside the flat-memory
test double `FlatMemory`. `read8`/`write8` are one exhaustive `match addr` (full
`u16` coverage, so a missing range is a compile error) that decodes the address
space:

- ROM (`0x0000-0x7FFF`) and external RAM (`0xA000-0xBFFF`) route to the
  `Cartridge` (and its MBC) via `Cartridge::read`/`write`.
- VRAM, WRAM, OAM, I/O, and HRAM are backed by RAM owned by `SystemBus`. Echo RAM
  (`0xE000-0xFDFF`) mirrors `0xC000-0xDDFF` (the `addr - 0xE000` index into WRAM
  produces exactly that partial mirror). The unusable region `0xFEA0-0xFEFF`
  reads `0xFF` and ignores writes.

`SystemBus` is the **real hardware home of the interrupt registers**: `IE`
(`0xFFFF`) and `IF` (`0xFF0F`) live in a small `InterruptRegisters` struct here.
The CPU's interrupt dispatch only knows their *addresses* and reaches them
through the `Bus`; the quirk that `IF`'s top three bits always read as `1` (and
are dropped on write) is encapsulated in `memory.rs`, not the CPU. This is what
lets the already-complete CPU dispatch run unchanged against the real bus — the
end-to-end test in `memory.rs` services a VBlank interrupt through `SystemBus`.

The timer (`0xFF04-0xFF07`) is real — see "Game Boy timer" below. `SystemBus`
also exposes `tick(t_states)`, which advances time-dependent subsystems (today
just the timer) by the T-cycles a just-executed instruction consumed and raises
the timer interrupt on overflow. There is no top-level loop calling it yet, so
it is test-driven for now.

The PPU is real too (see "Game Boy PPU" below): VRAM (`0x8000-0x9FFF`), OAM
(`0xFE00-0xFE9F`), and the LCD registers (`0xFF40-0xFF45`, `0xFF47-0xFF4B`) route
to the owned `Ppu`, which gates VRAM/OAM CPU access by mode and is advanced by
`SystemBus::tick`. OAM DMA (`0xFF46`) is real too — see "Game Boy OAM DMA" below.
The joypad (`0xFF00`) is real as well — see "Game Boy joypad" below. The serial
port (`0xFF01-0xFF02`) is real too — see "Game Boy serial" below. The APU
(`0xFF10-0xFF3F`) is real too — see "Game Boy APU" below.

Current stubs: every I/O register the machine defines now routes to a real
subsystem; the remaining flat read/write storage in the `io` array is only the
handful of genuinely-unimplemented/unused registers (e.g. `0xFF03`,
`0xFF08-0xFF0E`). Capturing the transmitted serial `SB` byte for headless
test-ROM output (the serial stub deliberately discards it) is still its own later
build-order item.

# Game Boy timer

`gameboy/src/timer.rs`'s `Timer` implements the DMG timer/divider
(`DIV`/`TIMA`/`TMA`/`TAC`, `0xFF04-0xFF07`) as a **falling-edge detector**, not a
free-running divider — the accurate model. A single 16-bit internal counter
increments once per T-cycle (`DIV` is its high byte); the timer picks one bit of
it (selected by `TAC`'s clock-select bits), ANDs it with the enable bit, and
ticks `TIMA` on each falling edge. This is what makes the obscure behaviors fall
out of one helper (`detect_falling_edge`): writing `DIV` (reset to 0) or `TAC`
(changing the selected bit/enable) can drop that signal 1→0 and spuriously tick
`TIMA`. `TIMA` overflow is **delayed one M-cycle** — `TIMA` reads `0x00` for
4 T-cycles, then `TMA` is loaded and the interrupt requested; a write to `TIMA`
during that window cancels the reload.

`Timer` knows nothing about interrupts: `Timer::tick(cycles) -> bool` returns
whether an overflow reload completed, and `SystemBus` (which owns the `Timer`)
turns that into `IF` bit 2 via `InterruptRegisters::request` — the on-bus
subsystem's analog of `Cpu::request_interrupt` (no whole-`Bus` re-borrow). The
`0xFF04-0xFF07` reads/writes route to the `Timer` through dedicated `match` arms
placed ahead of the flat I/O catch-all (the same ordering trick as `0xFF0F`).
Everything powers on zeroed; a post-boot `DIV`/`TAC` seed can be added later.
This timer is unrelated to the MBC3 RTC.

# Game Boy PPU

`gameboy/src/ppu.rs`'s `Ppu` is the DMG LCD controller. Like the timer it is a
self-contained subsystem owned by `SystemBus`: it **owns** VRAM (`0x8000-0x9FFF`),
OAM (`0xFE00-0xFE9F`), and the LCD registers (`LCDC`/`STAT`/`SCY`/`SCX`/`LY`/
`LYC`/`BGP`/`OBP0`/`OBP1`/`WY`/`WX`), and exposes `read_vram`/`write_vram`,
`read_oam`/`write_oam`, `read_register`/`write_register`, `step(cycles)`, and
`framebuffer()`. `SystemBus` routes those address ranges to it (dedicated `match`
arms ahead of the flat I/O catch-all, the same ordering trick as the timer).
`0xFF46` (OAM DMA) is **not** owned here — its controller lives in `dma.rs` (see
"Game Boy OAM DMA" below), though it writes OAM through the PPU's unguarded
`write_oam_dma`.

Timing is one dot per T-cycle: a frame is 154 lines of 456 dots, visible lines
cycling mode 2 (OAM scan, 80 dots) → mode 3 (drawing, a **fixed 172-dot
approximation** for now) → mode 0 (H-Blank), and lines 144-153 mode 1 (V-Blank).
`step` loops **one dot at a time** (like `Timer::tick`) so mode transitions and
the STAT rising-edge land on the exact dot even when an instruction advances the
clock by many T-cycles. It returns a `PpuStep { vblank, stat, frame_complete }`
struct (not the timer's bare `bool`, because the PPU emits two interrupts plus a
non-interrupt frame edge in one span); `SystemBus::tick` maps `vblank`/`stat` onto
`IF` via `PpuStep::interrupts()` and latches `frame_complete` for
`SystemBus::take_frame_ready()`. The **STAT interrupt is rising-edge only** over
the ORed enabled conditions (LYC=LY + mode 0/1/2 sources), so simultaneous
sources coalesce and a held-high source does not re-fire.

CPU access to VRAM/OAM is **mode-gated** but only while the LCD is on: mode 3
blocks VRAM (`read` returns `0xFF`, writes drop), modes 2-3 block OAM. Everything
powers on zeroed, so `LCDC` bit 7 is clear (LCD off) at boot and all of VRAM/OAM
is freely accessible until a game turns the LCD on.

Rendering is **scanline-based**: each visible line is drawn in full at the mode
3→0 transition into a `160×144` framebuffer of final shade indices (`0-3`, already
mapped through `BGP`/`OBP0`/`OBP1`). BG (signed/unsigned tile addressing, SCX/SCY
wrap), then window (an internal window-line counter that advances only on drawn
lines), then sprites (OAM scan capped at 10/line, 8×8 and 8×16, X/Y flip, OBP
palettes, color-0 transparency, DMG smaller-X-then-OAM-index priority, and the
OBJ-to-BG priority bit). The framebuffer feeds the generic `video` crate as a
`Grey2` buffer via `Gfx::update_frame` (which packs shades→2bpp with the DMG
palette); that seam is not yet driven because the top-level CPU+PPU run loop is a
later item. A pixel-FIFO renderer and per-dot-accurate mode-3 length are the
accurate-but-complex upgrades left for later.

# Game Boy OAM DMA

`gameboy/src/dma.rs`'s `OamDma` is the DMG OAM DMA controller (`0xFF46`). Like the
timer and PPU it is a self-contained subsystem owned by `SystemBus`, but it owns
**only timing/progress state** (`source_page`, `active`, `elapsed`,
`bytes_copied`) — it can't do the copy itself, since that needs both a bus read of
the source and an OAM write, which only `SystemBus` can orchestrate. Writing page
`XX` to `0xFF46` calls `OamDma::trigger` (a write mid-transfer restarts it); the
register reads back the last-written page. A transfer copies 160 bytes
`XX00-XX9F` → OAM over **160 M-cycles** (640 T-cycles), one byte per M-cycle.

`SystemBus::tick` advances it via `tick_dma`: `OamDma::advance(t_states)` bumps the
elapsed-cycle count and returns the half-open `[start, end)` range of byte indices
that came due (returning owned indices, not a borrow, so the `&mut dma` borrow is
dropped before the bus is re-read). `SystemBus` then copies each due byte with
`read_raw(base + i)` → `Ppu::write_oam_dma(i, byte)`. The OAM write uses the PPU's
unguarded `write_oam_dma` (bypassing the mode lock — DMA writes OAM regardless of
PPU mode), and the source read uses `read_raw` so the transfer isn't blocked by
its own bus hold.

Bus blocking: while a transfer is active the CPU can only reach HRAM. The
`Bus::read8`/`write8` methods are now thin **gates** — if `dma.is_active()` and the
address is not HRAM (`0xFF80-0xFFFE`) and not `0xFF46` itself, reads return `0xFF`
and writes are dropped — over the raw decode, which was extracted into private
`SystemBus::read_raw`/`write_raw`. The `0xFF46` register and its trigger stay
accessible mid-transfer. OAM DMA raises no interrupt.

There is no top-level CPU→`tick` loop yet, so (like the timer and PPU) this is
test-driven for now. First-pass simplifications: no startup/warmup delay (plain
640 T-cycles); blocked reads return `0xFF` rather than the exact in-flight
conflict byte; source pages `0xE0-0xFF` read straight through the bus decode
rather than being special-cased. This is the CPU/bus OAM DMA and is unrelated to
the `[CGB]` HDMA/GDMA (`0xFF51-0xFF55`).

# Game Boy joypad

`gameboy/src/joypad.rs`'s `Joypad` is the DMG joypad register `P1/JOYP`
(`0xFF00`). Like the timer/PPU/DMA it is a self-contained subsystem owned by
`SystemBus`: it owns the held-button bitfield and the last-written row-select
bits, and knows nothing about the interrupt registers. It models the 2×4 button
matrix, all **active-low** (`0` = pressed/selected): bits 7-6 read `1`; bit 5
(`P15`) `=0` selects the buttons row (Start/Select/B/A); bit 4 (`P14`) `=0`
selects the d-pad row (Down/Up/Left/Right); bits 3-0 report the selected row(s)
(both rows are ORed when both are selected). Only the two select bits are
writable. Everything powers on zeroed (boot ROM skipped), so both rows start
selected with nothing held and `read()` returns the conventional post-boot `0xCF`.

The button→bit layout is chosen so each hardware nibble is a direct slice of the
`pressed` field (low nibble = d-pad row, high nibble = buttons row, both already
in the register's bit3..0 order), so `read` composes with no per-button matching.
The API is `read()`, `write(value) -> bool`, and `set_pressed(&HashSet<GameboyButton>)
-> bool`. Both mutators return whether an input line fell `1→0` (a press on a
selected row, or a select change onto a held button) via one `detect_edge` helper
— the timer's return-a-bool interrupt seam. `SystemBus` routes `0xFF00` to it
(a dedicated arm ahead of the flat I/O catch-all, now `0xFF01..=0xFF7F`) and turns
a returned edge into `IF` bit 4 (`Interrupt::Joypad`, vector `$60`).

The frontend seam is `SystemBus::set_buttons(&HashSet<GameboyButton>)`, which the
future top-level run loop feeds each frame from the desktop input router's
`InputRouter::gameboy_pressed()` (types already line up). There is no such loop
yet, so — like the timer/PPU/DMA — the joypad is **test-driven for now**.

# Game Boy serial

`gameboy/src/serial.rs`'s `Serial` is the DMG serial port (`SB`/`SC`,
`0xFF01-0xFF02`) as a **single-player stub** — just enough of the link cable that
games which start a transfer and then poll for completion (or wait on the serial
interrupt) don't hang. Like the timer/joypad/PPU/DMA it is a self-contained
subsystem owned by `SystemBus`: it owns `SB`, `SC`, and a remaining-cycle
countdown, and knows nothing about the interrupt registers.

Only the **internal clock** drives a transfer: writing `SC` with bit 7 (start)
and bit 0 (clock source = internal) both set starts a timed transfer of one byte
over `4194304 / 8192 * 8 = 4096` T-cycles. On completion `SB` is loaded with
`0xFF` (the byte clocked in from a disconnected cable), `SC` bit 7 auto-clears,
and — via the timer's return-a-bool seam — `Serial::tick(cycles) -> bool` returns
`true` so `SystemBus::tick` requests `IF` bit 3 (`Interrupt::Serial`, vector
`$58`). A transfer started with the **external clock** (bit 0 = 0) waits for a
clock a disconnected cable never supplies, so — like real hardware with no partner
— it never completes. `SC` reads force its unused bits (6-1) to `1`, so an idle
`SC` reads the conventional post-boot `0x7E`; that forced value is also what proves
the register routes to `Serial` rather than the flat I/O array in tests. `0xFF01`/
`0xFF02` route to it via dedicated `match` arms ahead of the flat I/O catch-all
(now `0xFF03..=0xFF7F`), the same ordering trick as the timer/joypad.

Everything powers on zeroed (boot ROM skipped). There is no top-level run loop
calling `tick` yet, so — like the timer/PPU/DMA/joypad — the serial port is
**test-driven for now**. Out of scope (future items): capturing the transmitted
`SB` byte for headless test-ROM output (this stub discards it), real link-cable
emulation, and the `[CGB]` fast clock (`SC` bit 1) / double-speed timing.

# Game Boy APU

`gameboy/src/apu/` is the DMG audio processing unit (`0xFF10-0xFF3F`). Like the
timer/PPU/serial it is a self-contained subsystem owned by `SystemBus` and
advanced by `SystemBus::tick`, but unlike them it raises **no interrupt** — its
output is a stream of stereo audio samples. It is a directory (not a single
file) because it is materially larger than the other subsystems:

- `apu/components.rs` — the shared DSP helpers each channel reuses:
  `LengthCounter` (256 Hz, max 64 for square/noise and 256 for wave),
  `VolumeEnvelope` (64 Hz, DAC = top-5-bits-of-`NRx2` nonzero), and the
  channel-1 `Sweep` (128 Hz, with the immediate-overflow-on-trigger,
  disable-on-overflow, and negate-cleared-after-use quirks). Also the
  `dac_output` helper mapping a 4-bit digital sample to `[-1.0, 1.0]` (DAC off =
  analog silence).
- `apu/frame_sequencer.rs` — the 512 Hz `FrameSequencer`, an 8-step clock
  returning which of `{length, envelope, sweep}` fire per step (length 0/2/4/6,
  sweep 2/6, envelope 7). Stepped one T-cycle at a time like the timer.
- `apu/square.rs` — `SquareChannel`, used for **both** ch1 and ch2 via
  `sweep: Option<Sweep>`. Duty-cycle waveform table, frequency timer
  `(2048-freq)*4`.
- `apu/wave.rs` — `WaveChannel` + Wave RAM (`0xFF30-0xFF3F`, 32×4-bit). DAC is a
  single bit (`NR30` bit 7), output level is a coarse shift, no envelope. Wave
  RAM survives an APU power-off.
- `apu/noise.rs` — `NoiseChannel` with the 15/7-bit LFSR and the divisor table.
- `apu/mod.rs` — the `Apu`: owns the four channels + the frame sequencer +
  `NR50`/`NR51`/`NR52`. Handles register routing (per-register forced-1 read
  masks as named consts — they differ per register, unlike the timer/serial's
  single mask), the `NR52` power gate (bit 7 off clears `NR10-NR51` and the
  decoded channel state via a per-channel `reset()`, ignores subsequent register
  writes, but leaves Wave RAM live), the mixer, and sample generation.

Sample output: the channels are stepped one T-cycle at a time, and a fractional
accumulator emits one interleaved stereo `f32` pair every
`CLOCK_HZ / APU_SAMPLE_RATE` T-cycles at the fixed `APU_SAMPLE_RATE` (48 kHz).
`mix()` maps each channel's digital output through its DAC, sums per `NR51` L/R
routing, and scales by `NR50` master volume, staying within `[-1.0, 1.0]`.
`Apu::take_samples()` drains the buffer via `std::mem::take` — the audio analog
of the PPU's `take_frame_ready`/`framebuffer` seam. `SystemBus` routes
`0xFF10..=0xFF3F` to the `Apu` (dedicated `match` arm ahead of the flat I/O
catch-all, the same ordering trick as the timer/serial), calls `self.apu.tick`
in `tick`, and forwards the samples via `SystemBus::take_audio_samples`.
**Resampling** the fixed 48 kHz stream to the host device rate happens outside
this crate (in the `audio` crate) — the APU only produces the intermediate-rate
samples.

Power-on state is all-zero (boot ROM skipped); post-boot register *read* values
come from the forced-1 masks, not seeded storage (e.g. `NR52` reads `0x70` when
powered off). There is no top-level run loop calling `tick` yet, so — like the
other subsystems — the APU is **test-driven for now**. First-pass
simplifications (future refinements): frame-sequencer events are OR-ed
coarse-grained across a multi-T-cycle span; length counters are cleared on
power-off (a `[CGB]` behavior, not strictly DMG); Wave RAM access isn't
restricted to the narrow read window; the wave-trigger sample-buffer delay quirk
is not modeled.

# Audio output

Audio follows the same trait-in-`common`-agnostic / impl-in-a-dedicated-crate
split as video, so the real-device audio library stays isolated for headless
runs:

- **`common`:** `AudioSettings` (in `common::settings`, alongside
  `GraphicsSettings`): backend-agnostic, persisted prefs — `enabled: bool` and
  `volume: f32`. Anything wrapping a cpal type stays out of `common`, exactly as
  `Background(wgpu::Color)` stays out of it for video.
- **`audio` crate:** the concrete cpal implementation. `AudioOutput` (the trait
  a host drives, analog of `VideoOutput`) has one method, `submit(&[f32])`;
  `CpalAudioOutput` opens the default output device, resamples submitted samples
  from the source rate (`gameboy::APU_SAMPLE_RATE`) to the device rate through a
  streaming linear `Resampler` (in `audio::format`, pure and unit-tested), and
  feeds a shared `Mutex<VecDeque<f32>>` ring the cpal callback drains (silence
  when empty). A standalone `examples/tone.rs` exercises the device path without
  the emulator.
- **`emulator`:** `emulator/src/snd.rs`'s `Snd` is the host owner of the live
  stream (the audio analog of `Gfx`), constructed from `AudioSettings` in
  `App::resumed` — opening the device is non-fatal (a machine with no output
  device still runs silently), unlike a graphics-init failure. `Snd::submit_frame`
  is the seam a future run loop feeds with `SystemBus::take_audio_samples`;
  like `Gfx::update_frame` it is `#[allow(dead_code)]` until the top-level
  CPU+PPU+APU run loop lands, so the stream currently plays silence.

# UI overlay

The runtime menu overlays egui on top of the emulator video. It follows the same
agnostic-interface / concrete-backend split as video and audio, choosing the crate
for each piece by its coupling so a future frontend (e.g. a web app) reuses as much
as possible — egui splits cleanly along exactly this seam:

- **`ui` crate (`egui` core + `common` only):** the reusable UI. It owns
  `MenuState` (the `Screen` navigation + rebind-`Capture` state machine),
  `ui::draw(ctx, &mut state, &MenuData) -> Vec<MenuCommand>` (the screens: Root,
  Select ROM, Options, Input Bindings), the embedded `assets/Early GameBoy.ttf`
  font (`install_fonts`), and `trigger_label`. It is deliberately free of `wgpu`,
  `winit`, and `gameboy`: every system-specific value crosses the boundary as a
  plain `common`/`String` type (`RomChoice`, `BindingRow`, `BindingSet`), and every
  effect the UI can't perform leaves as a `MenuCommand` (`LoadRom`, `Exit`,
  `CloseMenu`, `SetBinding`). `cargo tree -p ui` shows neither wgpu nor winit — that
  is the reuse boundary, and it should stay that way.
- **`video::EguiRenderer` (`egui-wgpu`):** the GPU backend. It renders the
  host-tessellated `EguiPaint` (primitives + textures delta + pixels-per-point)
  into a `RenderTarget`, mirroring `PixelBufferRenderer`/`VideoOutput` so a wgpu
  host drives it the same way. It uses `LoadOp::Load` so the menu composits over
  the game rather than clearing it.
- **`emulator` (`egui-winit`):** the winit glue that can't be shared.
  `EguiPlatform` (`egui_platform.rs`) owns the `egui::Context` and
  `egui_winit::State`, translating winit events to egui input and tessellating
  `ui::draw`'s output each frame. `App` orchestrates: it forwards events to egui
  first and gates the game `InputRouter` on egui's `consumed` flag; the already-
  wired `GenericAction::Menu` (default Escape) now toggles the menu
  (`apply_generic_actions`); while the menu is open `drive_emulation` early-returns
  (the machine pauses) and `about_to_wait` pumps redraws itself (the run loop no
  longer does); menu `MenuCommand`s are applied in the redraw path
  (`load_rom` hot-swaps the cartridge; `apply_binding_edit` rebuilds the router and
  persists). Rebind capture routes the next raw key/mouse/gamepad input into
  `MenuState::capture` instead of the game.

Rendering detail: the game wants the sRGB surface for correct colors but egui wants
a linear target (it gamma-corrects itself), so `Gfx` adds the non-sRGB view format
to the surface `view_formats` and draws the egui pass through a linear *view* of the
same surface texture (`egui_format`).

Editable bindings persist through a `FileStore`-specific `save_input_bindings`
(writing both `[input]` and `[gameboy.input]`) rather than the `SettingsStore`
trait's `save_settings`, because the Game Boy bindings live outside
`common::Settings` (the same reason `[paths]` does). `ActionBindings::set` (in
`common`) is the by-name write path the editor uses.

# Style

Prefer to omit comments just for marking sections or describing obvious things. Only use comments when the code is actually complicated, but do use them in that case.

Prefer precise error types whenever possible. Don't `unwrap` or `expect` unless you really have to, ideally only at the top of the execution stack in main.

Prefer verbose logging, but use the levels appropriately. We can log a lot a trace, less at debug, and less still at info.