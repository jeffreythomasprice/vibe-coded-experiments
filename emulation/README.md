# emulation

A Rust workspace for building system emulators, starting with the original DMG
Game Boy. Crates live under `crates/`:

- `common` — shared utilities: logging init, the abstract persistent-state
  interface, and the shared video `ScaleMode`.
- `gameboy` — the Game Boy (SM83) emulator core. System-agnostic; no UI or
  filesystem dependencies.
- `video` — generic wgpu video output (scaling, letterboxing, pixel-format
  conversion) that UI shells drive to present a frame.
- `emulator` — the desktop UI shell (winit + wgpu) that hosts a core. Binary entry
  point.

## Building and running

```sh
cargo build
cargo run -p emulator -- [--config <path>] [rom]
```

- `rom` (optional) is a path to a Game Boy ROM to load and run.
- `--config <path>` (optional, also `-c`) overrides the settings file location.

With no ROM, the shell opens an empty window. `RUST_LOG` overrides the default log
filter (our crates at `trace`, everything else at `warn`).

## Listing ROMs

To see what the emulator finds in the configured ROMs directory, run the
`list-roms` subcommand. It **scans the roms dir recursively** — ROMs nested in
sub-directories count — and prints each one as its path relative to the roms dir,
annotated with the cartridge header title when the ROM carries one:

```sh
cargo run -p emulator -- list-roms [--config <path>]
```

The names it prints are exactly what `run <name>` matches against (file name, file
stem, or header title).

## Configuration

The emulator keeps persistent state under a per-user config directory:

- Linux: `~/.config/emulator/`
- macOS: `~/Library/Application Support/emulator/`

Three kinds of state live there:

| State | Location | Notes |
| --- | --- | --- |
| Settings | `settings.toml` | Graphics scale mode; input bindings. |
| Saves | `saves/` | Battery RAM (and, later, save states). Configurable. |
| ROMs | `roms/` | Where a future in-app ROM browser looks. Configurable. |

### Creating `settings.toml`

On first run the emulator **writes a default `settings.toml` automatically**, and
creates the `saves/` and `roms/` directories. To start from a documented template
instead, copy [`settings.example.toml`](settings.example.toml):

```sh
mkdir -p ~/.config/emulator
cp settings.example.toml ~/.config/emulator/settings.toml
```

The `saves_dir` and `roms_dir` keys under `[paths]` are optional; when omitted they
default to `saves/` and `roms/` next to the config file. See the example file for
all keys and their meanings.

### Video scale mode

`[graphics] scale_mode` selects how the emulated 160×144 frame is scaled into the
window: `"original"`, `"fit"` (default), `"stretch"`, or `{ scaled = 4.0 }` for a
fixed factor. The value is applied to the renderer at startup.

### Input bindings

Keyboard, mouse, and gamepad inputs are bound to actions under `[input]` (generic
actions shared by every core, e.g. `menu`) and `[gameboy.input]` (the Game Boy's
buttons). Each action takes an **array** of triggers, so several inputs can drive
it. Defaults: d-pad = arrow keys, `X` = A, `Z` = B, `Enter` = Start, `Right Shift`
= Select, and `Escape` = menu.

An action **omitted** from the file uses its default; an action set to an **empty
array** (`up = []`) is deliberately unbound. Because the defaults live in code, the
file can be empty and everything still works.

To extract the full set of defaults — every action written out explicitly — run the
`print-config` subcommand, which prints a complete `settings.toml` to stdout and
exits without opening a window:

```sh
cargo run -p emulator -- print-config > ~/.config/emulator/settings.toml
```

Edit the result to customize, e.g. rebinding A to the keyboard's `a` key:

```toml
[gameboy.input]
a = [{ key = "key_a" }, { gamepad_button = "east" }]
```

## Running the test ROMs

The `gameboy` crate ships a headless harness (`gameboy::testrom`) that runs the
standard [Blargg hardware test ROMs](https://github.com/jeffreythomasprice/gb-test-roms)
to a pass/fail verdict. It watches both channels those ROMs report through: the
**serial port** (Blargg prints its output to the link port, ending in `Passed`/
`Failed`) and a **result block in cartridge RAM at `$A000`** (used by the ROMs that
print nothing, e.g. `mem_timing-2`).

The ROMs are expected at a fixed location baked into `testrom::TEST_ROMS_DIR`:

```
/home/jeff/workspaces/personal/gb-test-roms
```

Clone them there to run the tests:

```sh
git clone https://github.com/jeffreythomasprice/gb-test-roms \
  /home/jeff/workspaces/personal/gb-test-roms
```

### As a test suite

```sh
cargo test -p gameboy
```

The active tests run `cpu_instrs` (the combined ROM and all 11 individual
sub-tests) and `instr_timing` and assert they pass, plus unit tests for both
detection paths. If the ROM checkout is missing, the ROM tests **skip with a
warning** rather than fail.

Some tests are marked `#[ignore]` (run them with `cargo test -p gameboy -- --ignored`):

- `mem_timing` / `mem_timing_2` — check sub-instruction memory-access timing, which
  the instruction-stepped CPU does not model yet.
- `oam_bug` — the OAM-bug hardware quirk is not modelled by the PPU yet.

### As an example app

`examples/run_rom.rs` runs a single ROM either headless or in a live window, always
flat-out (no real-time pacing). `<rom>` is a path relative to `TEST_ROMS_DIR`, or
an absolute/working-directory path.

```sh
# Windowed (default): watch the ROM's output render live.
cargo run -p gameboy --example run_rom -- "cpu_instrs/individual/06-ld r,r.gb"

# Headless, and save a PNG of the final frame.
cargo run -p gameboy --example run_rom -- cpu_instrs/cpu_instrs.gb --headless --screenshot
```

Flags: `--headless` (default: windowed) and `--screenshot[=path]` (default: off;
omit the path to derive `<rom-stem>.png`). The process exits non-zero on a
failed/timed-out ROM.

## Battery saves

Battery-backed cartridge RAM is persisted through the storage layer into
`saves/<title>.sav`, where `<title>` is derived from the cartridge header title
(falling back to the ROM's file name). **This differs from many emulators, which
put the `.sav` next to the ROM** — existing side-by-side `.sav` files are not read.
