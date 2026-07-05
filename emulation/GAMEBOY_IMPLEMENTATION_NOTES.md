# Game Boy Emulator — Implementation Notes

Working notes distilled from the docs in `references/gameboy/`, organized by the
subsystems we need to build. These are **summaries meant to break ground** — each
section links back to the source document so we can dig for detail later.

Scope for the first pass is the **original DMG Game Boy** (per `TODO.md`). CGB
(Color) behavior is captured inline where the source docs mix it in, but it is
tagged **[CGB — future]** and can be ignored for the initial emulator.

---

## Reference index

| Source file (`references/gameboy/`) | What it covers |
| --- | --- |
| `Specifications.html` | Master doc (Pan Docs / Martin Korth mirror). Memory map, sound, joypad, serial, interrupts, CGB registers, CPU registers & flags, full opcode timing tables, cartridge header, MBCs, power-up sequence. This is the primary reference for most subsystems. |
| `Game Boy CPU (SM83) instruction set.htm` | The full opcode matrix (unprefixed + `$CB` prefixed): mnemonic, length, T-state cycles, flags affected. Use this to build the CPU decode/dispatch. |
| `GBCPUman.pdf` | Full CPU manual (per-instruction prose descriptions). Deepest reference for exact instruction semantics. |
| `Video Display - GbdevWiki.html` | The PPU in detail: LCDC/STAT, modes & dot timing, scrolling, palettes, VRAM tile/map layout, OAM sprite attributes, sprite priority, VRAM/OAM access windows, OAM DMA. |
| `Timer and Divider Registers - GbdevWiki.html` | DIV/TIMA/TMA/TAC basics. |
| `Timer Obscure Behaviour - GbdevWiki.html` | Timer edge cases (falling-edge detector, TIMA overflow delay, write glitches) — needed to pass timer test ROMs. |
| `README.md` | External links: opcode tables (`gbdev.io/gb-opcodes/optables/`), test ROMs (`gb-test-roms`), `mgbdis` disassembler. |

External (from `README.md`), not local but worth knowing:
- Opcode tables: https://gbdev.io/gb-opcodes/optables/ (also linked as JSON from the instruction-set page)
- Test ROMs: https://github.com/jeffreythomasprice/gb-test-roms (Blargg / mooneye style)
- `mgbdis`: https://github.com/mattcurrie/mgbdis (disassembler; can consume symbol files our emulator could emit)

Modern living version of the Pan Docs: https://gbdev.io/pandocs/ (the local HTML files are the older wiki mirror; when something is ambiguous, cross-check there).

---

## System overview & timing model

*Source: `Specifications.html` (CPU specs), `Video Display - GbdevWiki.html` (dot clock).*

- **CPU:** Sharp SM83 — an 8-bit core, roughly an 8080 with some Z80 features and
  `$CB`-prefixed bit ops. Not a full Z80 (no IX/IY, no alt register set, no
  IN/OUT). See "CPU" below.
- **Master clock:** 4.194304 MHz (`2^22` Hz). This is the **dot clock** (a.k.a.
  "T-states" / "T-cycles").
- **Machine cycle (M-cycle):** 4 T-states. Every instruction's duration is a
  multiple of 4 T-states, so many docs quote timings "÷4" as M-cycles. Memory
  accesses happen 1 per M-cycle.
- **The whole machine is driven by this one clock.** The cleanest model: a
  central scheduler steps the CPU one instruction (or sub-step) at a time, and
  every other subsystem (PPU, timer, APU, DMA, serial) is "ticked" by the number
  of cycles that instruction consumed. Getting this cycle accounting right is
  what makes test ROMs pass.
- **[CGB — future] Double-speed mode** (`KEY1`/`FF4D`): CPU, timer/divider,
  serial, and OAM DMA run 2× faster; PPU, HDMA, and all sound run unchanged.

**Recommended stepping granularity:** for the first pass, step one full
instruction, then advance the other subsystems by that instruction's cycle count
("instruction-stepped"). This passes most tests. A later refinement is
"cycle-stepped" (advance subsystems 4 T-states at a time during each memory
access), which is needed for the hardest PPU/timing tests.

---

## CPU (SM83 core)

*Source: `Specifications.html` (CPU Registers and Flags; CPU Instruction Set;
CPU Comparison with Z80), `Game Boy CPU (SM83) instruction set.htm` (opcode
matrix), `GBCPUman.pdf` (per-instruction semantics).*

### Registers
- 8-bit: `A F B C D E H L`. Paired as 16-bit: `AF BC DE HL`.
- 16-bit: `SP` (stack pointer), `PC` (program counter).
- `F` (flags) is the low byte of `AF`. Only the upper 4 bits are used; **bits
  3-0 are always zero** (must be masked on any write to `F`, e.g. via `POP AF`).

### Flags (`F` register)
| Bit | Flag | Meaning |
| --- | --- | --- |
| 7 | `Z` | Zero — result was 0 |
| 6 | `N` | Add/Sub — set if last op was a subtraction (for DAA) |
| 5 | `H` | Half-carry — carry out of bit 3 (for DAA) |
| 4 | `C` | Carry — carry out of bit 7 (8-bit) / bit 15 (16-bit), or shifted-out bit |

`DAA` is the only consumer of `N`/`H`. `H` is fiddly (per-instruction); the
opcode page's flag column (`Z N H C`) is authoritative for each instruction.

### Instruction set
- ~256 base opcodes + 256 `$CB`-prefixed opcodes.
- Build a decode table (a big match/dispatch on the opcode byte, with a nested
  match for the `$CB` prefix). The instruction-set HTML gives, for every opcode:
  **mnemonic, byte length, T-state cycles, and flags affected** — mirror it
  directly.
- Groups: 8-bit loads, 16-bit loads, 8-bit ALU (add/adc/sub/sbc/and/or/xor/cp/
  inc/dec/daa/cpl), 16-bit ALU (add hl,rr / inc/dec rr / add sp,r8 / ld hl,sp+r8),
  rotates & shifts (rlca/rla/rrca/rra + CB rlc/rl/rrc/rr/sla/sra/swap/srl),
  single-bit ops (CB bit/set/res), control (nop/halt/stop/di/ei/ccf/scf),
  jumps/calls/returns/rst.
- **Conditional** jumps/calls/returns take **more cycles when the branch is
  taken** — the opcode page shows this as `taken/not-taken` (e.g. `20/8`).
- **Immediates:** `d8` = 8-bit imm, `d16` = little-endian 16-bit imm, `a16` =
  little-endian 16-bit address, `a8` = 8-bit added to `$FF00` (I/O access),
  `r8` = signed 8-bit relative offset.
- **`$FF00+` I/O ops** replace the Z80's IN/OUT: `LDH (a8),A` / `LDH A,(a8)` /
  `LD (C),A` / `LD A,(C)`.
- The undefined opcodes (`D3 DB DD E3 E4 EB EC ED F4 FC FD`) **lock up the CPU**
  on real hardware — treat as a hard fault/panic in the emulator.

### Control-flow quirks that matter for tests
- **`HALT` (`$76`):** CPU sleeps until an interrupt is pending
  (`IE & IF != 0`). The infamous **"halt bug":** if `IME=0` and an interrupt is
  already pending when `HALT` executes, the CPU does *not* halt and the byte
  after `HALT` is read twice (PC fails to increment once). Needed for some tests.
- **`STOP` (`$10`):** encoded as two bytes (`$10 00`); the second byte is
  ignored. Enters very-low-power standby; on DMG woken by joypad. **[CGB]** also
  triggers the speed switch when `KEY1` bit 0 is set.
- **`EI`** enables interrupts with a **one-instruction delay** (IME becomes set
  *after* the instruction following `EI`). `DI` disables immediately. `RETI` =
  `RET` + immediate IME enable.

### Init state after boot
See "Boot / power-up" below for the exact register/IO values to seed if we skip
the boot ROM.

---

## Memory map & bus

*Source: `Specifications.html` (Memory Map).*

| Range | Region | Notes |
| --- | --- | --- |
| `0000-3FFF` | ROM bank 00 | Fixed (cartridge). Also the MBC control write-target. |
| `4000-7FFF` | ROM bank 01..NN | Switchable via MBC. |
| `8000-9FFF` | VRAM (8 KB) | Tile data + BG maps. Inaccessible to CPU during PPU mode 3. **[CGB]** 2 banks. |
| `A000-BFFF` | External (cartridge) RAM | Switchable bank, if the cart has RAM. Often battery-backed. |
| `C000-CFFF` | WRAM bank 0 (4 KB) | |
| `D000-DFFF` | WRAM bank 1 (4 KB) | **[CGB]** banks 1-7 switchable. |
| `E000-FDFF` | Echo RAM | Mirror of `C000-DDFF`. "Do not use," but emulate the mirror. |
| `FE00-FE9F` | OAM (sprite attribute table) | 40 sprites × 4 bytes. Inaccessible during PPU modes 2-3. |
| `FEA0-FEFF` | Unusable | Reads/writes have quirky behavior; return `$FF` / ignore for now. |
| `FF00-FF7F` | I/O registers | Joypad, serial, timer, sound, LCD, etc. (see per-subsystem sections). |
| `FF80-FFFE` | HRAM (High RAM, 127 bytes) | Usable during OAM DMA (see DMA). |
| `FFFF` | `IE` — Interrupt Enable | |

**Interrupt / RST vectors** live low in ROM: `RST` targets `00 08 10 18 20 28 30
38`; interrupt vectors `40 48 50 58 60`.

Implementation: a central `read8(addr)` / `write8(addr)` bus that dispatches by
range to cartridge/MBC, VRAM, WRAM, OAM, I/O registers, and HRAM. PPU/DMA gating
of VRAM/OAM reads (returning `$FF`, ignoring writes) can be added incrementally.

---

## Cartridge & Memory Bank Controllers (MBC)

*Source: `Specifications.html` (The Cartridge Header; Memory Bank Controllers).*

### Cartridge header (`0100-014F`)
Key fields to parse:
- `0100-0103` **Entry point** — usually `NOP; JP $0150`.
- `0104-0133` **Nintendo logo** — boot ROM verifies this bitmap; wrong bytes
  lock a real GB. (We can ignore verification unless emulating the boot ROM.)
- `0134-0143` **Title** (ASCII, upper-case, `00`-padded). **[CGB]** shortened.
- `0143` **CGB flag** (`80`=CGB-enhanced, `C0`=CGB-only).
- `0146` **SGB flag** (`03`=supports SGB).
- `0147` **Cartridge type** — which MBC + extras (RAM/BATTERY/TIMER/RUMBLE).
- `0148` **ROM size** — `32KB << N` (N=0..8 gives 2..256 banks).
- `0149` **RAM size** — `00`=none, `01`=2KB, `02`=8KB, `03`=32KB (4×8KB).
- `014A` Destination, `014B` old licensee, `014C` mask ROM version.
- `014D` **Header checksum:** `x=0; for i in 0134..014C: x = x - MEM[i] - 1`;
  low byte must match. (Real GB refuses to boot if wrong — we can just parse.)
- `014E-014F` **Global checksum** (16-bit, sum of all ROM bytes; GB never checks it).

### Cartridge type IDs (`0147`) — most common
`00` ROM only · `01-03` MBC1(+RAM/+BATTERY) · `05-06` MBC2(+BATTERY) ·
`0F-13` MBC3(+TIMER/+RAM/+BATTERY) · `19-1E` MBC5(+RAM/+RUMBLE/+BATTERY).
Full table in the source.

### MBCs to implement (in priority order)
The `A000-BFFF` region and `0000-7FFF` writes are how the MBC is controlled
(writes to ROM space are *commands*, not data).

- **None / ROM-only (`00`):** ROM straight-mapped `0000-7FFF`; optional 8KB RAM
  at `A000-BFFF`. Start here.
- **MBC1** (≤2MB ROM / ≤32KB RAM). Control registers via ROM-space writes:
  - `0000-1FFF` RAM enable (write `$0A` low-nibble to enable).
  - `2000-3FFF` ROM bank low 5 bits (writing `0` → `1`; banks `20/40/60` alias
    to `21/41/61`).
  - `4000-5FFF` 2-bit register: upper ROM bank bits **or** RAM bank.
  - `6000-7FFF` mode select (0 = ROM banking, 1 = RAM banking).
- **MBC2** (≤256KB ROM, built-in 512×4-bit RAM at `A000-A1FF`). Enable/bank
  select distinguished by **bit 8 of the address** (`0000-1FFF`: bit8=0 → RAM
  enable, bit8=1 → ROM bank). Only lower 4 bits of its RAM bytes are valid.
- **MBC3** (≤2MB ROM / ≤32KB RAM + **RTC**). Like MBC1 but 7-bit ROM bank in
  `2000-3FFF` (no forbidden banks); `4000-5FFF` selects RAM bank `00-03` **or**
  RTC register `08-0C`; `6000-7FFF` latch-clock (`00`→`01` latches time). RTC
  registers: S/M/H/DL/DH (day counter + halt + carry). RTC is independent of the
  built-in timer (see Timer section).
- **MBC5** — needed for many late/CGB titles; add after the above.

**Battery RAM / save:** carts flagged `+BATTERY` persist `A000-BFFF` RAM. This
maps to our persistent-state system (save files) — see `TODO.md`.

---

## Interrupts

*Source: `Specifications.html` (Interrupts).*

Five interrupts, sharing three pieces of state:
- **`IME`** — Interrupt Master Enable (a CPU flag, not memory-mapped). Set by
  `EI`/`RETI`, cleared by `DI` and automatically when an interrupt is serviced.
- **`IE` (`FFFF`)** — per-interrupt enable bits.
- **`IF` (`FF0F`)** — per-interrupt request/pending bits.

| Bit | Interrupt | Vector | Priority |
| --- | --- | --- | --- |
| 0 | V-Blank | `$40` | highest |
| 1 | LCD STAT | `$48` | |
| 2 | Timer | `$50` | |
| 3 | Serial | `$58` | |
| 4 | Joypad | `$60` | lowest |

**Dispatch logic** (checked between instructions): if `IME` and `(IE & IF)` has
any bit set, pick the lowest-set bit (highest priority), then:
1. clear `IME`,
2. clear that bit in `IF`,
3. push `PC` (2 bytes) to the stack,
4. jump `PC` to the vector,
5. costs ~20 T-states (5 M-cycles).

A pending interrupt (`IE & IF`) also wakes the CPU from `HALT` even when
`IME=0` (but then does *not* service it if `IME=0` — see the halt bug above).

---

## Timer & Divider

*Source: `Timer and Divider Registers - GbdevWiki.html`,
`Timer Obscure Behaviour - GbdevWiki.html`, `Specifications.html`.*

Registers:
- **`DIV` (`FF04`)** — upper 8 bits of a **16-bit internal counter** that always
  increments at the CPU clock. Visible `DIV` increments at 16384 Hz. **Any write
  resets the whole internal counter to 0.** Not gated by the timer-enable bit.
- **`TIMA` (`FF05`)** — timer counter. Increments at the rate selected by `TAC`.
  On overflow (>`$FF`) it reloads from `TMA` and requests the Timer interrupt.
- **`TMA` (`FF06`)** — modulo; value loaded into `TIMA` on overflow.
- **`TAC` (`FF07`)** — bit 2 = timer enable; bits 1-0 = clock select:
  `00`=4096 Hz (÷1024), `01`=262144 Hz (÷16), `10`=65536 Hz (÷64),
  `11`=16384 Hz (÷256).

**Correct model:** the timer is a **falling-edge detector on a specific bit of
the internal 16-bit counter** (the bit chosen by `TAC`), ANDed with the enable
bit — *not* a separate free-running divider. This is what makes the obscure
behaviors fall out naturally:
- Writing to `DIV` (resetting the counter) can spuriously tick `TIMA` if the
  selected bit was 1 (falling edge).
- Changing `TAC` can tick `TIMA` for the same reason.
- **TIMA overflow is delayed 1 M-cycle:** for one cycle after overflow `TIMA`
  reads `$00` (not `TMA`), and *then* `TMA` is loaded and `IF` timer bit set.
  Writing `TIMA` during that window cancels the reload/interrupt; a write during
  the reload cycle is overridden by `TMA`.

For a first pass, a simpler counter model works for most games; implement the
edge-detector model to pass the timer test ROMs. Note: this timer is unrelated
to the MBC3 RTC.

---

## PPU / Video (LCD controller)

*Source: `Video Display - GbdevWiki.html` (primary),
`Specifications.html` (Video I/O Registers).*

Screen: **160×144** visible pixels. BG map is 256×256 (32×32 tiles). Per `TODO.md`,
the PPU owns a grid of output pixels that a generic wgpu adapter blits to a
texture.

### Frame timing (the heart of the PPU)
- Dot clock = 4.194304 MHz. A frame = **154 scanlines**, **70224 dots**
  (~59.7 Hz). Each visible line (LY 0-143) is **456 dots**, cycling modes
  2 → 3 → 0. Lines 144-153 are mode 1 (V-Blank).
- **Modes** (low 2 bits of `STAT`):
  - **Mode 2** — OAM scan (80 dots). Find up to 10 sprites overlapping this line.
  - **Mode 3** — drawing (≈168-291 dots, variable; extends via SCX%8, window,
    sprites). VRAM **and** OAM inaccessible to CPU.
  - **Mode 0** — H-Blank (remainder of the 456). VRAM/OAM accessible.
  - **Mode 1** — V-Blank (10 lines). VRAM/OAM accessible; fires V-Blank interrupt
    at LY=144.
- **CPU VRAM/OAM access gating:** during mode 3 VRAM reads return `$FF` / writes
  ignored; during modes 2-3 the same for OAM. First pass can skip gating (games
  mostly respect it), but tests check it.

For the first pass, a **scanline-based renderer** (render each line when it
completes, advancing the mode/dot counters per instruction) is enough. A
pixel-FIFO renderer is the accurate-but-complex upgrade.

### Registers
- **`LCDC` (`FF40`)** — master control:
  bit7 LCD enable · bit6 window tile-map (`9800`/`9C00`) · bit5 window enable ·
  bit4 BG/window tile-data area (`8800`/`8000`) · bit3 BG tile-map · bit2 OBJ
  size (8×8 / 8×16) · bit1 OBJ enable · bit0 BG/window enable-priority.
- **`STAT` (`FF41`)** — bit6 LYC=LY int enable · bit5 mode2 int · bit4 mode1 int ·
  bit3 mode0 int · bit2 coincidence flag (LYC==LY, read-only) · bits1-0 mode.
  The STAT interrupt fires on the **rising edge** of "any enabled condition met"
  (a source of missed/extra interrupts to be careful about).
- **`SCY`/`SCX` (`FF42`/`FF43`)** — BG scroll (wraps around the 256×256 map).
- **`LY` (`FF44`)** — current scanline (read-only, 0-153).
- **`LYC` (`FF45`)** — compared against LY → coincidence flag / STAT int.
- **`WY`/`WX` (`FF4A`/`FF4B`)** — window position; on-screen top-left is
  `(WX-7, WY)`. `WX` 0-6 and 166 are buggy.
- **`BGP` (`FF47`)** — BG palette: 4 × 2-bit shade indices (0=white … 3=black).
- **`OBP0`/`OBP1` (`FF48`/`FF49`)** — sprite palettes; color 0 = transparent
  (low 2 bits unused).
- **[CGB — future]** `VBK` (`FF4F`) VRAM bank; `BCPS/BCPD` `OCPS/OCPD`
  (`FF68-FF6B`) color palette RAM; `BG map attributes` in VRAM bank 1.

### VRAM layout
- **Tile data `8000-97FF`:** 384 tiles × 16 bytes. Each tile = 8×8 px, 2 bits/px
  (4 shades). Each row = 2 bytes: byte0 = low bits of each pixel, byte1 = high
  bits; **bit 7 = leftmost pixel**. Two addressing modes (LCDC bit4):
  "8000" = unsigned base `$8000`; "8800" = signed base `$9000`. Sprites always
  use 8000/unsigned; BG/window pick via LCDC.
- **BG maps `9800-9BFF` and `9C00-9FFF`:** two 32×32 byte maps, each byte a tile
  index into the tile-data table.
- **Window:** a second, non-scrollable BG layer positioned by `WX`/`WY`, sharing
  the same tile data.

### Sprites (OAM, `FE00-FE9F`)
40 entries × 4 bytes: **Y** (screen Y + 16), **X** (screen X + 8), **tile #**
(8×16 mode ignores low bit), **attributes** (bit7 OBJ-behind-BG priority, bit6 Y
flip, bit5 X flip, bit4 palette OBP0/1 **[DMG]**, **[CGB]** bit3 bank / bits2-0
palette).
- **10-sprites-per-line limit**, decided during OAM scan.
- **DMG priority:** among the 10, smaller X wins; ties broken by OAM index.
  **[CGB]** priority is purely OAM index.
- Sprite color 0 is transparent. `OBJ-to-BG priority` bit: when set, BG colors
  1-3 draw over the sprite (BG color 0 always behind).

---

## OAM DMA & VRAM DMA

*Source: `Video Display - GbdevWiki.html` (LCD OAM/VRAM DMA),
`Specifications.html`.*

- **OAM DMA (`FF46`):** writing value `XX` copies `XX00-XX9F` → OAM
  (`FE00-FE9F`), 160 bytes over **160 M-cycles**. During the transfer the CPU can
  only touch **HRAM** (`FF80-FFFE`) — which is why the DMA routine is copied into
  and run from HRAM. This is the normal way games update sprites (usually in
  V-Blank). Implement as a timed copy that blocks non-HRAM bus access.
- **[CGB — future] HDMA/GDMA (`FF51-FF55`):** VRAM DMA, either all-at-once
  (general purpose, halts CPU) or 16 bytes per H-Blank. Skip for DMG.

---

## Joypad / Input

*Source: `Specifications.html` (Joypad Input).*

- **`P1/JOYP` (`FF00`)** is a 2×4 matrix. Write to select a row, read low nibble:
  - bit5 (`P15`) = 0 selects **buttons** (Start/Select/B/A),
  - bit4 (`P14`) = 0 selects **d-pad** (Down/Up/Left/Right),
  - bits3-0 = the four inputs, **0 = pressed** (active-low). Bits 7-6 unused.
- Mapping when selected: bit3 Down/Start · bit2 Up/Select · bit1 Left/B ·
  bit0 Right/A.
- **Joypad interrupt (INT `$60`)** fires on a high→low transition (a press). On
  DMG it's mainly used to wake from `STOP`. Games typically poll `FF00` instead.

Per `TODO.md`, the generic input/event system maps host keyboard/gamepad events
to this register state (and optionally raises the interrupt). Read the docs' note
that programs read the port several times to debounce.

---

## Serial / Link Cable

*Source: `Specifications.html` (Serial Data Transfer).*

- **`SB` (`FF01`)** — 8-bit shift data. **`SC` (`FF02`)** — control: bit7 start
  (auto-clears when done), bit0 clock source (1=internal, 0=external),
  **[CGB]** bit1 fast clock.
- Transfer shifts 8 bits **MSB-first**; sending and receiving happen
  simultaneously. Internal clock on DMG = 8192 Hz (~1 KB/s).
- On completion → **Serial interrupt (INT `$58`)**.
- With no link partner and internal clock, the received byte is `$FF`.

For a single-player emulator, implement the timing + "receive `$FF`, raise the
interrupt" stub so games that poll serial don't hang. Real linking is later.

---

## Sound / APU

*Source: `Specifications.html` (Sound Controller).*

4 channels, mixed into 2 stereo terminals (SO1/SO2). Master 4.194304 MHz clock
drives a **frame sequencer** (512 Hz) that clocks length counters (256 Hz),
volume envelopes (64 Hz), and channel-1 sweep (128 Hz).

- **Channel 1 — square + sweep + envelope** (`NR10-NR14`, `FF10-FF14`).
  Frequency `= 131072/(2048-x)` Hz; sweep shifts frequency; duty cycle 12.5/25/
  50/75%.
- **Channel 2 — square + envelope** (`NR21-NR24`, `FF16-FF19`). Like ch1, no sweep.
- **Channel 3 — wave** (`NR30-NR34`, `FF1A-FF1E`) + **Wave RAM `FF30-FF3F`**
  (32 × 4-bit samples). Output-level shift (mute/100/50/25%).
- **Channel 4 — noise** (`NR41-NR44`, `FF20-FF23`). LFSR-based; 15-bit or 7-bit
  width (bit3 of `NR43`) changes tone/noise character.
- **Control:** `NR50` (`FF24`) master L/R volume + Vin routing; `NR51` (`FF25`)
  per-channel L/R routing; `NR52` (`FF26`) bit7 all-sound on/off + read-only
  per-channel status. Disabling via `NR52` bit7 clears all sound registers.

Each channel: bit7 of its `NRx4` "initial/restart" triggers the channel; bit6
"length enable" stops it when its length counter expires; envelopes step volume.

**Priority for our build:** audio can come **after** CPU/PPU/timers are solid
(games are playable without it). Then implement the frame sequencer + the 4
channels, resampling the mixed output to the host audio rate.

---

## Boot / power-up

*Source: `Specifications.html` (Power Up Sequence).*

Real hardware runs a 256-byte internal boot ROM (scrolls the Nintendo logo,
verifies logo + header checksum, then hands off to `$0100`). We have two options:

1. **Skip the boot ROM** (simplest): jump straight to `$0100` and seed the exact
   post-boot state. DMG values:
   - `AF=$01B0  BC=$0013  DE=$00D8  HL=$014D  SP=$FFFE`, `PC=$0100`.
   - I/O seed (subset): `TIMA/TMA/TAC=$00`; sound regs `NR10..NR52` per the table
     (`FF10=$80 … FF26=$F1`); `LCDC(FF40)=$91`; `SCY/SCX/LYC=$00`; `BGP(FF47)=$FC`;
     `OBP0/OBP1=$FF`; `WY/WX=$00`; `IE(FFFF)=$00`. Full list in the source.
   - Note the doc's own caveat: don't over-rely on exact values; the important
     ones for correctness are the CPU registers and `LCDC`/palettes.
2. **Run a real boot ROM** later for accuracy (needs the logo verification, so
   the cart header logo bytes must be correct).

WRAM/VRAM/OAM come up with random contents on hardware; emulators conventionally
zero them.

---

## Suggested build order & testing

*Source: `README.md` (test ROMs + tooling), `TODO.md` (project structure).*

Rough dependency order for breaking ground (each item is independently testable):

1. **Cartridge loader + header parse** (ROM-only first). Bus with ROM/WRAM/HRAM.
2. **CPU core** — registers, flags, the full opcode + `$CB` dispatch, cycle
   counts, `EI` delay, halt/stop. Seed post-boot state (skip boot ROM).
3. **Interrupts** (`IME`/`IE`/`IF`, dispatch, vectors).
4. **Timer/Divider** (edge-detector model).
5. **PPU** — start with LCDC/STAT/LY/mode timing + V-Blank interrupt, then BG
   rendering, then window, then sprites. Wire the pixel grid into the generic
   wgpu video adapter described in `TODO.md`.
6. **OAM DMA** (`FF46`).
7. **Joypad** (`FF00`) via the generic input system.
8. **MBC1**, then **MBC3** (+RTC), then **MBC5**; battery-RAM saves.
9. **Serial** stub (so polling games don't hang).
10. **APU** (frame sequencer + 4 channels) last.

**Testing strategy** (matches `TODO.md`): run the `gb-test-roms` (Blargg's
`cpu_instrs`, `instr_timing`, `mem_timing`, `dmg-acid2` for PPU, mooneye timer
tests, etc.). These emit pass/fail either to the **serial port** (capture `SB`
writes) or to the **screen**. `TODO.md` suggests OCR-ing the screen output; the
easier hook is to capture serial output for the ROMs that use it. Wire captured
serial/screen output into unit-test assertions so regressions are caught
automatically. `mgbdis` can disassemble ROMs (and consume symbol files our
emulator could emit) when debugging.

**Cross-check reference for exact per-opcode behavior:** the opcode JSON linked
from `Game Boy CPU (SM83) instruction set.htm`, the online tables at
`gbdev.io/gb-opcodes/optables/`, and `GBCPUman.pdf` for prose semantics.
