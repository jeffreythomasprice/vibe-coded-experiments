//! The Game Boy (DMG) PPU — the LCD controller.
//!
//! [`Ppu`] owns the pixel pipeline: the 8 KiB of VRAM (`0x8000-0x9FFF`, tile
//! data + background maps), the 160-byte OAM sprite table (`0xFE00-0xFE9F`), and
//! the LCD registers (`LCDC`/`STAT`/`SCY`/`SCX`/`LY`/`LYC`/`BGP`/`OBP0`/`OBP1`/
//! `WY`/`WX`, `0xFF40-0xFF4B` except the `0xFF46` OAM-DMA port). It mirrors the
//! [`Timer`](crate::timer::Timer) pattern: a self-contained subsystem with
//! register `read`/`write`, a `step(cycles)` dot-clock advance, and a
//! return-value seam ([`PpuStep`]) that reports interrupts to its owner
//! ([`SystemBus`](crate::memory::SystemBus)) rather than reaching into the
//! interrupt registers itself.
//!
//! ## Timing model
//! One dot == one T-cycle (DMG's dot clock is the CPU clock). A frame is 154
//! scanlines of 456 dots. Visible lines (`LY` 0-143) cycle mode 2 (OAM scan,
//! 80 dots) → mode 3 (drawing, a fixed 172-dot approximation) → mode 0 (H-Blank,
//! the remaining 204). Lines 144-153 are mode 1 (V-Blank). [`Ppu::step`] loops
//! one dot at a time (like the timer) so mode transitions and the STAT
//! rising-edge are evaluated at the exact dot they occur, even when an
//! instruction advances the clock by many T-cycles at once.
//!
//! ## Rendering
//! A **scanline renderer**: each visible line is drawn in full at the moment
//! mode 3 ends, reading whatever is in VRAM/OAM at that instant. The output is a
//! 160×144 grid of final shade indices (`0-3`, already mapped through
//! `BGP`/`OBP0`/`OBP1`), which the `video` crate blits with the DMG palette. A
//! pixel-FIFO renderer is the accurate-but-complex upgrade left for later.
//!
//! ## CPU access gating
//! While the LCD is on, VRAM is inaccessible to the CPU during mode 3 (reads
//! return `0xFF`, writes are dropped) and OAM during modes 2-3. When the LCD is
//! off (the power-on state, since everything comes up zeroed) all of it is
//! freely accessible.

use crate::cpu::Interrupt;

const VRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0x00A0;
const SCREEN_W: usize = 160;
const SCREEN_H: usize = 144;
const FB_SIZE: usize = SCREEN_W * SCREEN_H;

const DOTS_PER_LINE: u16 = 456;
const LINES_PER_FRAME: u8 = 154;
const VBLANK_START_LINE: u8 = 144;

const MODE2_DOTS: u16 = 80; // OAM scan
const MODE3_DOTS: u16 = 172; // drawing (fixed approximation)
const MODE3_END: u16 = MODE2_DOTS + MODE3_DOTS; // 252; H-Blank fills the rest

const MAX_SPRITES_PER_LINE: usize = 10;

// LCDC (0xFF40) bits.
const LCDC_ENABLE: u8 = 1 << 7;
const LCDC_WIN_MAP: u8 = 1 << 6; // 0 = 0x9800, 1 = 0x9C00
const LCDC_WIN_ENABLE: u8 = 1 << 5;
const LCDC_TILE_DATA: u8 = 1 << 4; // 1 = 0x8000 unsigned, 0 = 0x8800 signed
const LCDC_BG_MAP: u8 = 1 << 3; // 0 = 0x9800, 1 = 0x9C00
const LCDC_OBJ_SIZE: u8 = 1 << 2; // 0 = 8x8, 1 = 8x16
const LCDC_OBJ_ENABLE: u8 = 1 << 1;
const LCDC_BG_ENABLE: u8 = 1 << 0;

// STAT (0xFF41) bits.
const STAT_LYC_INT: u8 = 1 << 6;
const STAT_MODE2_INT: u8 = 1 << 5;
const STAT_MODE1_INT: u8 = 1 << 4;
const STAT_MODE0_INT: u8 = 1 << 3;
const STAT_COINCIDENCE: u8 = 1 << 2;
const STAT_WRITABLE_MASK: u8 = 0x78; // bits 3-6 are the only writable STAT bits
const STAT_UNUSED: u8 = 0x80; // bit 7 always reads back as 1

// OAM sprite attribute (byte 3) bits.
const OBJ_ATTR_PRIORITY: u8 = 1 << 7; // 1 = behind BG colors 1-3
const OBJ_ATTR_YFLIP: u8 = 1 << 6;
const OBJ_ATTR_XFLIP: u8 = 1 << 5;
const OBJ_ATTR_PALETTE: u8 = 1 << 4; // 0 = OBP0, 1 = OBP1

/// A PPU rendering mode, occupying the low two bits of `STAT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    HBlank,
    VBlank,
    OamScan,
    Drawing,
}

impl Mode {
    fn bits(self) -> u8 {
        match self {
            Mode::HBlank => 0,
            Mode::VBlank => 1,
            Mode::OamScan => 2,
            Mode::Drawing => 3,
        }
    }
}

/// What a [`Ppu::step`] span produced. Mirrors the timer's return-value seam,
/// but the PPU emits three independent edges in one span — the two interrupts
/// (VBlank on `IF` bit 0, STAT/LCD on bit 1) and a "frame is ready to present"
/// signal that is not an interrupt at all — so it is a small struct rather than
/// the timer's bare `bool`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PpuStep {
    /// The V-Blank interrupt should be raised (the PPU entered line 144).
    pub vblank: bool,
    /// The STAT/LCD interrupt should be raised (rising edge of the ORed enabled
    /// STAT conditions).
    pub stat: bool,
    /// A full frame finished during this span; the framebuffer is coherent.
    pub frame_complete: bool,
}

impl PpuStep {
    fn merge(&mut self, other: PpuStep) {
        self.vblank |= other.vblank;
        self.stat |= other.stat;
        self.frame_complete |= other.frame_complete;
    }
}

/// The DMG PPU. See the module docs for the timing and rendering model.
#[derive(Debug)]
pub struct Ppu {
    vram: Box<[u8; VRAM_SIZE]>,
    oam: Box<[u8; OAM_SIZE]>,
    framebuffer: Box<[u8; FB_SIZE]>,

    lcdc: u8,
    /// Only the writable bits (3-6) are stored; the mode (bits 0-1) and
    /// coincidence flag (bit 2) are synthesized on read.
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,

    dot: u16,
    mode: Mode,
    /// Advances only on scanlines where the window was actually drawn, so the
    /// window is independent of `LY`/`SCY`.
    window_line: u8,
    /// Previous value of the ORed STAT condition, for rising-edge detection.
    stat_line: bool,
    frame_complete: bool,

    /// The current line's BG/window color id (`0-3`, before palette mapping),
    /// kept so sprites can honor the OBJ-to-BG priority bit.
    bg_color_id: [u8; SCREEN_W],
}

impl Default for Ppu {
    fn default() -> Ppu {
        Ppu::new()
    }
}

impl Ppu {
    /// A PPU in its power-on state: everything zeroed. `LCDC = 0` means the LCD
    /// is off, matching how [`SystemBus`](crate::memory::SystemBus) powers the
    /// rest of the machine up zeroed (we skip the boot ROM).
    pub fn new() -> Ppu {
        tracing::debug!("ppu initialized");
        Ppu {
            vram: Box::new([0; VRAM_SIZE]),
            oam: Box::new([0; OAM_SIZE]),
            framebuffer: Box::new([0; FB_SIZE]),
            lcdc: 0,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            dot: 0,
            mode: Mode::HBlank,
            window_line: 0,
            stat_line: false,
            frame_complete: false,
            bg_color_id: [0; SCREEN_W],
        }
    }

    fn lcd_on(&self) -> bool {
        self.lcdc & LCDC_ENABLE != 0
    }

    fn vram_locked(&self) -> bool {
        self.lcd_on() && self.mode == Mode::Drawing
    }

    fn oam_locked(&self) -> bool {
        self.lcd_on() && matches!(self.mode, Mode::OamScan | Mode::Drawing)
    }

    /// Read VRAM (`0x8000-0x9FFF`). Returns `0xFF` while the PPU is drawing
    /// (mode 3) with the LCD on.
    pub fn read_vram(&self, addr: u16) -> u8 {
        if self.vram_locked() {
            0xFF
        } else {
            self.vram[(addr - 0x8000) as usize]
        }
    }

    /// Write VRAM (`0x8000-0x9FFF`). Dropped while the PPU is drawing (mode 3)
    /// with the LCD on.
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        if !self.vram_locked() {
            self.vram[(addr - 0x8000) as usize] = value;
        }
    }

    /// Read OAM (`0xFE00-0xFE9F`). Returns `0xFF` during modes 2-3 with the LCD
    /// on.
    pub fn read_oam(&self, addr: u16) -> u8 {
        if self.oam_locked() {
            0xFF
        } else {
            self.oam[(addr - 0xFE00) as usize]
        }
    }

    /// Write OAM (`0xFE00-0xFE9F`). Dropped during modes 2-3 with the LCD on.
    pub fn write_oam(&mut self, addr: u16, value: u8) {
        if !self.oam_locked() {
            self.oam[(addr - 0xFE00) as usize] = value;
        }
    }

    /// Write OAM byte `index` (`0..0xA0`) from an OAM DMA transfer, **bypassing**
    /// the mode lock: DMA writes land regardless of PPU mode (unlike the
    /// CPU-facing, mode-gated [`Ppu::write_oam`]). The bus drives this while the
    /// DMA controller ([`OamDma`](crate::dma::OamDma)) is active.
    pub fn write_oam_dma(&mut self, index: u8, value: u8) {
        self.oam[index as usize] = value;
    }

    /// Read an LCD register (`0xFF40-0xFF45`, `0xFF47-0xFF4B`). `0xFF46`
    /// (OAM DMA) is handled by the bus, not here.
    pub fn read_register(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => {
                let coincidence = if self.lcd_on() && self.ly == self.lyc {
                    STAT_COINCIDENCE
                } else {
                    0
                };
                let mode = if self.lcd_on() { self.mode.bits() } else { 0 };
                STAT_UNUSED | (self.stat & STAT_WRITABLE_MASK) | coincidence | mode
            }
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => unreachable!("ppu register read of non-ppu address {addr:#06x}"),
        }
    }

    /// Write an LCD register (`0xFF40-0xFF45`, `0xFF47-0xFF4B`). `LY` is
    /// read-only; `STAT`'s low three bits are read-only.
    pub fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF40 => {
                let was_on = self.lcd_on();
                self.lcdc = value;
                if was_on && !self.lcd_on() {
                    self.disable_lcd();
                }
            }
            0xFF41 => self.stat = value & STAT_WRITABLE_MASK,
            0xFF42 => self.scy = value,
            0xFF43 => self.scx = value,
            0xFF44 => {} // LY is read-only.
            0xFF45 => self.lyc = value,
            0xFF47 => self.bgp = value,
            0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy = value,
            0xFF4B => self.wx = value,
            _ => unreachable!("ppu register write of non-ppu address {addr:#06x}"),
        }
    }

    /// The current framebuffer: `160 * 144` shade indices (`0-3`), row-major.
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer[..]
    }

    /// Take and clear the "a frame completed" latch, for a UI loop deciding when
    /// to present.
    pub fn take_frame_complete(&mut self) -> bool {
        std::mem::take(&mut self.frame_complete)
    }

    /// Advance the PPU by `cycles` dots (T-cycles). Returns the interrupts and
    /// frame edge produced. Does nothing while the LCD is off.
    pub fn step(&mut self, cycles: u32) -> PpuStep {
        let mut result = PpuStep::default();
        if !self.lcd_on() {
            return result;
        }
        for _ in 0..cycles {
            result.merge(self.tick_one_dot());
        }
        result
    }

    fn tick_one_dot(&mut self) -> PpuStep {
        let mut result = PpuStep::default();

        self.dot += 1;
        if self.dot == DOTS_PER_LINE {
            self.dot = 0;
            self.ly += 1;
            if self.ly == LINES_PER_FRAME {
                self.ly = 0;
                self.window_line = 0;
            }
        }

        let new_mode = if self.ly >= VBLANK_START_LINE {
            Mode::VBlank
        } else if self.dot < MODE2_DOTS {
            Mode::OamScan
        } else if self.dot < MODE3_END {
            Mode::Drawing
        } else {
            Mode::HBlank
        };

        if new_mode != self.mode {
            // Draw the visible scanline the instant mode 3 finishes.
            if new_mode == Mode::HBlank {
                self.render_scanline();
            }
            // The V-Blank interrupt (and the frame edge) fire once, on entry to
            // line 144.
            if new_mode == Mode::VBlank && self.ly == VBLANK_START_LINE {
                result.vblank = true;
                result.frame_complete = true;
                self.frame_complete = true;
                tracing::trace!("ppu entered vblank; frame complete");
            }
            self.mode = new_mode;
        }

        self.update_stat_edge(&mut result);
        result
    }

    /// Reset the timing state when the LCD is switched off. Hardware parks at
    /// `LY = 0`, mode 0, and restarts from the top when re-enabled.
    fn disable_lcd(&mut self) {
        self.ly = 0;
        self.dot = 0;
        self.mode = Mode::HBlank;
        self.window_line = 0;
        self.stat_line = false;
        tracing::trace!("ppu lcd disabled");
    }

    /// The ORed STAT condition: any enabled interrupt source currently met.
    fn stat_condition(&self) -> bool {
        let coincidence = self.ly == self.lyc;
        (self.stat & STAT_LYC_INT != 0 && coincidence)
            || (self.mode == Mode::OamScan && self.stat & STAT_MODE2_INT != 0)
            || (self.mode == Mode::VBlank && self.stat & STAT_MODE1_INT != 0)
            || (self.mode == Mode::HBlank && self.stat & STAT_MODE0_INT != 0)
    }

    /// The STAT interrupt fires only on the rising edge of [`stat_condition`],
    /// so simultaneous sources coalesce into one interrupt and a held-high
    /// source does not re-fire.
    ///
    /// [`stat_condition`]: Ppu::stat_condition
    fn update_stat_edge(&mut self, result: &mut PpuStep) {
        let condition = self.stat_condition();
        if condition && !self.stat_line {
            result.stat = true;
        }
        self.stat_line = condition;
    }

    fn render_scanline(&mut self) {
        self.render_background();
        if self.render_window() {
            self.window_line += 1;
        }
        self.render_sprites();
    }

    /// Byte offset into `vram` of the 16-byte tile with the given map index,
    /// honoring the `LCDC` tile-data addressing mode. `0x8000` maps to index 0
    /// and `0x9000` to index `0x1000`.
    fn tile_data_offset(&self, tile_index: u8) -> usize {
        if self.lcdc & LCDC_TILE_DATA != 0 {
            tile_index as usize * 16
        } else {
            (0x1000 + (tile_index as i8 as isize) * 16) as usize
        }
    }

    /// Combine the two bitplanes of a tile row into the 2-bit color at `bit`
    /// (`bit` is `7 - column` for an unflipped pixel).
    fn tile_color(lo: u8, hi: u8, bit: usize) -> u8 {
        (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1)
    }

    fn render_background(&mut self) {
        let base = self.ly as usize * SCREEN_W;
        if self.lcdc & LCDC_BG_ENABLE == 0 {
            // With BG/window disabled the whole layer reads as shade 0.
            for x in 0..SCREEN_W {
                self.framebuffer[base + x] = 0;
                self.bg_color_id[x] = 0;
            }
            return;
        }

        let map_base = if self.lcdc & LCDC_BG_MAP != 0 { 0x1C00 } else { 0x1800 };
        let y = self.ly.wrapping_add(self.scy);
        let tile_row = (y / 8) as usize;
        let row_in_tile = (y % 8) as usize;

        for x in 0..SCREEN_W {
            let map_x = (x as u8).wrapping_add(self.scx);
            let tile_col = (map_x / 8) as usize;
            let bit = 7 - (map_x % 8) as usize;
            let tile_index = self.vram[map_base + tile_row * 32 + tile_col];
            let addr = self.tile_data_offset(tile_index) + row_in_tile * 2;
            let color_id = Self::tile_color(self.vram[addr], self.vram[addr + 1], bit);
            self.bg_color_id[x] = color_id;
            self.framebuffer[base + x] = (self.bgp >> (color_id * 2)) & 0x3;
        }
    }

    /// Overlay the window on this scanline. Returns whether it drew anything (so
    /// the caller advances the internal window line counter).
    fn render_window(&mut self) -> bool {
        if self.lcdc & LCDC_BG_ENABLE == 0
            || self.lcdc & LCDC_WIN_ENABLE == 0
            || self.wy > self.ly
            || self.wx > 166
        {
            return false;
        }

        let base = self.ly as usize * SCREEN_W;
        let map_base = if self.lcdc & LCDC_WIN_MAP != 0 { 0x1C00 } else { 0x1800 };
        let win_y = self.window_line as usize;
        let tile_row = win_y / 8;
        let row_in_tile = win_y % 8;
        let left = self.wx as i16 - 7;

        for x in 0..SCREEN_W {
            if (x as i16) < left {
                continue;
            }
            let win_x = (x as i16 - left) as usize;
            let tile_col = win_x / 8;
            let bit = 7 - (win_x % 8);
            let tile_index = self.vram[map_base + tile_row * 32 + tile_col];
            let addr = self.tile_data_offset(tile_index) + row_in_tile * 2;
            let color_id = Self::tile_color(self.vram[addr], self.vram[addr + 1], bit);
            self.bg_color_id[x] = color_id;
            self.framebuffer[base + x] = (self.bgp >> (color_id * 2)) & 0x3;
        }
        true
    }

    fn render_sprites(&mut self) {
        if self.lcdc & LCDC_OBJ_ENABLE == 0 {
            return;
        }
        let height: i16 = if self.lcdc & LCDC_OBJ_SIZE != 0 { 16 } else { 8 };
        let ly = self.ly as i16;

        // OAM scan: the first ten sprites (in OAM order) that cover this line.
        let mut chosen = [0usize; MAX_SPRITES_PER_LINE];
        let mut count = 0;
        for i in 0..40 {
            let sy = self.oam[i * 4] as i16 - 16;
            if ly >= sy && ly < sy + height {
                chosen[count] = i;
                count += 1;
                if count == MAX_SPRITES_PER_LINE {
                    break;
                }
            }
        }

        // DMG priority: smaller X wins, ties broken by OAM index. Draw lowest
        // priority first so higher priority overwrites it.
        let chosen = &mut chosen[..count];
        chosen.sort_by_key(|&i| (self.oam[i * 4 + 1], i));

        let base = self.ly as usize * SCREEN_W;
        for &i in chosen.iter().rev() {
            let sy = self.oam[i * 4] as i16 - 16;
            let sx = self.oam[i * 4 + 1] as i16 - 8;
            let mut tile = self.oam[i * 4 + 2];
            let attr = self.oam[i * 4 + 3];
            if height == 16 {
                tile &= 0xFE;
            }

            let mut row = (ly - sy) as u8;
            if attr & OBJ_ATTR_YFLIP != 0 {
                row = (height as u8 - 1) - row;
            }
            let addr = tile as usize * 16 + row as usize * 2;
            let lo = self.vram[addr];
            let hi = self.vram[addr + 1];
            let palette = if attr & OBJ_ATTR_PALETTE != 0 { self.obp1 } else { self.obp0 };

            for col in 0..8i16 {
                let bit = if attr & OBJ_ATTR_XFLIP != 0 { col } else { 7 - col } as usize;
                let color_id = Self::tile_color(lo, hi, bit);
                if color_id == 0 {
                    continue; // sprite color 0 is transparent
                }
                let x = sx + col;
                if x < 0 || x >= SCREEN_W as i16 {
                    continue;
                }
                let x = x as usize;
                // OBJ-to-BG priority: when set, BG colors 1-3 stay in front.
                if attr & OBJ_ATTR_PRIORITY != 0 && self.bg_color_id[x] != 0 {
                    continue;
                }
                self.framebuffer[base + x] = (palette >> (color_id * 2)) & 0x3;
            }
        }
    }
}

/// The V-Blank and STAT interrupt sources a [`PpuStep`] maps to, exposed so the
/// owning bus can translate the result into `IF` requests without duplicating
/// the mapping.
impl PpuStep {
    /// The interrupts this step requests, in `(source, requested)` pairs.
    pub fn interrupts(self) -> [(Interrupt, bool); 2] {
        [(Interrupt::VBlank, self.vblank), (Interrupt::Lcd, self.stat)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PPU with the LCD enabled and nothing else configured.
    fn ppu_on() -> Ppu {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, LCDC_ENABLE);
        ppu
    }

    /// Write a tile's 16 bytes at `vram[tile_index * 16]` from eight `(lo, hi)`
    /// bitplane row pairs.
    fn put_tile(ppu: &mut Ppu, tile_index: usize, rows: [(u8, u8); 8]) {
        let base = tile_index * 16;
        for (r, (lo, hi)) in rows.iter().enumerate() {
            ppu.vram[base + r * 2] = *lo;
            ppu.vram[base + r * 2 + 1] = *hi;
        }
    }

    /// A tile that is a solid `color` (0-3) in every pixel.
    fn solid_tile(color: u8) -> [(u8, u8); 8] {
        let lo = if color & 1 != 0 { 0xFF } else { 0x00 };
        let hi = if color & 2 != 0 { 0xFF } else { 0x00 };
        [(lo, hi); 8]
    }

    fn put_sprite(ppu: &mut Ppu, oam_index: usize, y: u8, x: u8, tile: u8, attr: u8) {
        ppu.oam[oam_index * 4] = y;
        ppu.oam[oam_index * 4 + 1] = x;
        ppu.oam[oam_index * 4 + 2] = tile;
        ppu.oam[oam_index * 4 + 3] = attr;
    }

    /// The shade at screen `(x, y)`.
    fn pixel(ppu: &Ppu, x: usize, y: usize) -> u8 {
        ppu.framebuffer()[y * SCREEN_W + x]
    }

    /// Step `n` dots, returning the merged result.
    fn run_dots(ppu: &mut Ppu, n: u32) -> PpuStep {
        ppu.step(n)
    }

    #[test]
    fn power_on_state_is_lcd_off_mode_zero_ly_zero() {
        let ppu = Ppu::new();
        assert!(!ppu.lcd_on());
        assert_eq!(ppu.read_register(0xFF44), 0); // LY
        assert_eq!(ppu.read_register(0xFF41) & 0x3, 0); // mode reads 0 while off
    }

    #[test]
    fn lcdc_reads_and_writes_all_eight_bits() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xAB);
        assert_eq!(ppu.read_register(0xFF40), 0xAB);
    }

    #[test]
    fn stat_low_bits_are_read_only_and_derived() {
        let mut ppu = ppu_on();
        // Try to write every bit; only 3-6 stick, bit 7 reads 1, low 3 derived.
        ppu.write_register(0xFF41, 0xFF);
        let stat = ppu.read_register(0xFF41);
        assert_eq!(stat & STAT_WRITABLE_MASK, STAT_WRITABLE_MASK); // 3-6 stored
        assert_eq!(stat & STAT_UNUSED, STAT_UNUSED); // bit 7 forced 1
        // Low three bits reflect live mode/coincidence, not the 0xFF we wrote.
        assert_eq!(stat & 0x3, ppu.mode.bits());
    }

    #[test]
    fn stat_bit7_reads_as_one() {
        let ppu = Ppu::new();
        assert_eq!(ppu.read_register(0xFF41) & STAT_UNUSED, STAT_UNUSED);
    }

    #[test]
    fn stat_reports_current_mode_in_low_bits() {
        let mut ppu = ppu_on(); // starts mode HBlank (bits 0)
        run_dots(&mut ppu, 1); // dot 1 → Oam scan
        assert_eq!(ppu.read_register(0xFF41) & 0x3, Mode::OamScan.bits());
        run_dots(&mut ppu, MODE2_DOTS as u32); // into drawing
        assert_eq!(ppu.read_register(0xFF41) & 0x3, Mode::Drawing.bits());
    }

    #[test]
    fn lcdc_scroll_palette_round_trip() {
        let mut ppu = Ppu::new();
        for (addr, value) in [
            (0xFF42u16, 0x12u8), // SCY
            (0xFF43, 0x34),      // SCX
            (0xFF45, 0x56),      // LYC
            (0xFF47, 0xE4),      // BGP
            (0xFF48, 0x1B),      // OBP0
            (0xFF49, 0x24),      // OBP1
            (0xFF4A, 0x78),      // WY
            (0xFF4B, 0x9A),      // WX
        ] {
            ppu.write_register(addr, value);
            assert_eq!(ppu.read_register(addr), value, "register {addr:#06x}");
        }
    }

    #[test]
    fn ly_is_read_only() {
        let mut ppu = ppu_on();
        ppu.write_register(0xFF44, 0x50);
        assert_eq!(ppu.read_register(0xFF44), 0);
    }

    #[test]
    fn lyc_coincidence_flag_tracks_ly() {
        let mut ppu = ppu_on();
        ppu.write_register(0xFF45, 1); // LYC = 1
        assert_eq!(ppu.read_register(0xFF41) & STAT_COINCIDENCE, 0); // LY 0 != 1
        run_dots(&mut ppu, DOTS_PER_LINE as u32); // advance to LY 1
        assert_eq!(ppu.read_register(0xFF44), 1);
        assert_eq!(ppu.read_register(0xFF41) & STAT_COINCIDENCE, STAT_COINCIDENCE);
    }

    #[test]
    fn mode_progresses_oam_drawing_hblank_within_a_line() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, 1);
        assert_eq!(ppu.mode, Mode::OamScan);
        run_dots(&mut ppu, (MODE2_DOTS - 1) as u32); // reach dot 80
        assert_eq!(ppu.mode, Mode::Drawing);
        run_dots(&mut ppu, MODE3_DOTS as u32); // reach dot 252
        assert_eq!(ppu.mode, Mode::HBlank);
    }

    #[test]
    fn ly_increments_every_456_dots() {
        let mut ppu = ppu_on();
        assert_eq!(ppu.read_register(0xFF44), 0);
        run_dots(&mut ppu, DOTS_PER_LINE as u32);
        assert_eq!(ppu.read_register(0xFF44), 1);
        run_dots(&mut ppu, DOTS_PER_LINE as u32);
        assert_eq!(ppu.read_register(0xFF44), 2);
    }

    #[test]
    fn frame_is_154_lines() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, DOTS_PER_LINE as u32 * LINES_PER_FRAME as u32);
        assert_eq!(ppu.read_register(0xFF44), 0); // wrapped back to line 0
    }

    #[test]
    fn vblank_interrupt_fires_entering_line_144() {
        let mut ppu = ppu_on();
        // Step to just before line 144.
        let before = run_dots(&mut ppu, DOTS_PER_LINE as u32 * VBLANK_START_LINE as u32 - 1);
        assert!(!before.vblank);
        let entering = run_dots(&mut ppu, 1);
        assert_eq!(ppu.read_register(0xFF44), VBLANK_START_LINE);
        assert!(entering.vblank);
        assert_eq!(ppu.mode, Mode::VBlank);
        // It does not re-fire while V-Blank continues.
        let during = run_dots(&mut ppu, DOTS_PER_LINE as u32);
        assert!(!during.vblank);
    }

    #[test]
    fn frame_complete_latches_at_vblank() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, DOTS_PER_LINE as u32 * VBLANK_START_LINE as u32);
        assert!(ppu.take_frame_complete());
        assert!(!ppu.take_frame_complete()); // cleared by the take
    }

    #[test]
    fn stat_interrupt_fires_on_rising_edge_of_lyc_coincidence() {
        let mut ppu = ppu_on();
        ppu.write_register(0xFF45, 1); // LYC = 1
        ppu.write_register(0xFF41, STAT_LYC_INT); // enable LYC=LY STAT source
        let line0 = run_dots(&mut ppu, DOTS_PER_LINE as u32 - 1); // still LY 0
        assert!(!line0.stat);
        let cross = run_dots(&mut ppu, 1); // LY becomes 1 → coincidence rises
        assert_eq!(ppu.read_register(0xFF44), 1);
        assert!(cross.stat);
    }

    #[test]
    fn stat_interrupt_does_not_refire_without_a_falling_edge() {
        let mut ppu = ppu_on();
        // Enable both the mode-2 and LYC sources; LYC=0 so coincidence holds at
        // line 0. The condition stays high across several dots.
        ppu.write_register(0xFF45, 0);
        ppu.write_register(0xFF41, STAT_LYC_INT | STAT_MODE2_INT);
        let first = run_dots(&mut ppu, 1); // condition rises
        assert!(first.stat);
        let again = run_dots(&mut ppu, 10); // still high — no new edge
        assert!(!again.stat);
    }

    #[test]
    fn lcd_disabled_holds_ly_zero_and_mode_zero() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, DOTS_PER_LINE as u32 * 3 + 100); // somewhere in line 3
        assert_ne!(ppu.read_register(0xFF44), 0);
        ppu.write_register(0xFF40, 0); // LCD off
        assert_eq!(ppu.read_register(0xFF44), 0);
        assert_eq!(ppu.read_register(0xFF41) & 0x3, 0);
        // Stepping does nothing while off.
        let step = run_dots(&mut ppu, DOTS_PER_LINE as u32 * 10);
        assert_eq!(step, PpuStep::default());
        assert_eq!(ppu.read_register(0xFF44), 0);
    }

    #[test]
    fn vram_blocked_during_mode3_when_lcd_on() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, MODE2_DOTS as u32 + 1); // into drawing (mode 3)
        assert_eq!(ppu.mode, Mode::Drawing);
        ppu.write_vram(0x8000, 0xAB); // dropped
        assert_eq!(ppu.read_vram(0x8000), 0xFF); // blocked read
    }

    #[test]
    fn oam_blocked_during_modes_2_and_3_when_lcd_on() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, 1); // mode 2 (OAM scan)
        assert_eq!(ppu.mode, Mode::OamScan);
        ppu.write_oam(0xFE00, 0xAB); // dropped
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);
        run_dots(&mut ppu, MODE2_DOTS as u32); // mode 3
        assert_eq!(ppu.mode, Mode::Drawing);
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);
    }

    #[test]
    fn write_oam_dma_bypasses_the_mode_lock() {
        let mut ppu = ppu_on();
        run_dots(&mut ppu, MODE2_DOTS as u32); // advance into mode 3
        assert_eq!(ppu.mode, Mode::Drawing); // write_oam would be dropped here
        ppu.write_oam_dma(0, 0xAB);
        ppu.write_oam_dma(0x9F, 0xCD);
        // The bytes landed despite the lock (read directly; read_oam is gated).
        assert_eq!(ppu.oam[0], 0xAB);
        assert_eq!(ppu.oam[0x9F], 0xCD);
    }

    #[test]
    fn vram_and_oam_accessible_when_lcd_off() {
        let mut ppu = Ppu::new(); // LCD off
        ppu.write_vram(0x8000, 0xAB);
        assert_eq!(ppu.read_vram(0x8000), 0xAB);
        ppu.write_oam(0xFE00, 0xCD);
        assert_eq!(ppu.read_oam(0xFE00), 0xCD);
    }

    /// Render the current LY by stepping one full line's worth of dots.
    fn render_line(ppu: &mut Ppu) {
        run_dots(ppu, DOTS_PER_LINE as u32);
    }

    #[test]
    fn bg_renders_a_solid_tile_through_bgp() {
        let mut ppu = Ppu::new();
        // Tile 1 is solid color 2; map entry (0,0) points at it. Identity BGP.
        put_tile(&mut ppu, 1, solid_tile(2));
        ppu.vram[0x1800] = 1; // BG map 0x9800 entry 0
        ppu.write_register(0xFF47, 0b11_10_01_00); // BGP: color n → shade n
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE);
        render_line(&mut ppu); // renders LY 0
        for x in 0..8 {
            assert_eq!(pixel(&ppu, x, 0), 2, "x={x}");
        }
    }

    #[test]
    fn bg_maps_colors_through_a_nonidentity_bgp() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(1));
        ppu.vram[0x1800] = 1;
        ppu.write_register(0xFF47, 0b00_00_11_00); // color 1 → shade 3
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE);
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 3);
    }

    #[test]
    fn bg_8800_signed_tile_addressing() {
        let mut ppu = Ppu::new();
        // In signed mode, tile index 0 is at 0x9000 == vram offset 0x1000.
        let base = 0x1000;
        for r in 0..8 {
            ppu.vram[base + r * 2] = 0xFF; // color 1 rows
            ppu.vram[base + r * 2 + 1] = 0x00;
        }
        ppu.vram[0x1800] = 0; // map entry → tile 0
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_BG_ENABLE); // TILE_DATA clear → signed
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 1);
    }

    #[test]
    fn bg_scroll_wraps() {
        let mut ppu = Ppu::new();
        // Put a distinctive tile at map entry (0,0) and a different one at (0,1).
        put_tile(&mut ppu, 1, solid_tile(1));
        put_tile(&mut ppu, 2, solid_tile(2));
        ppu.vram[0x1800] = 1; // (col 0)
        ppu.vram[0x1800 + 1] = 2; // (col 1)
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF43, 8); // SCX = 8 → screen x0 samples map col 1
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE);
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 2); // shows the col-1 tile
    }

    #[test]
    fn bg_disabled_renders_shade_zero() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(3));
        ppu.vram[0x1800] = 1;
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA); // BG enable clear
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 0);
    }

    #[test]
    fn window_overlays_bg_at_wx_wy() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(1)); // BG tile
        put_tile(&mut ppu, 2, solid_tile(3)); // window tile
        for e in 0..32 {
            ppu.vram[0x1800 + e] = 1; // BG map all tile 1
            ppu.vram[0x1C00 + e] = 2; // window map (0x9C00) all tile 3
        }
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF4A, 0); // WY = 0
        ppu.write_register(0xFF4B, 7); // WX = 7 → window at screen x 0
        ppu.write_register(
            0xFF40,
            LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE | LCDC_WIN_ENABLE | LCDC_WIN_MAP,
        );
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 3); // window shade, not BG
    }

    #[test]
    fn window_line_counter_advances_only_when_drawn() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(1));
        // Window tile row 0 is color 3, row 1 is color 1 — so we can tell which
        // internal window line was used.
        let mut win = solid_tile(3);
        win[1] = (0xFF, 0x00); // row 1 → color 1
        put_tile(&mut ppu, 2, win);
        for e in 0..32 {
            ppu.vram[0x1C00 + e] = 2;
        }
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF4A, 2); // WY = 2: window starts on line 2
        ppu.write_register(0xFF4B, 7); // WX = 7
        ppu.write_register(
            0xFF40,
            LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE | LCDC_WIN_ENABLE | LCDC_WIN_MAP,
        );
        render_line(&mut ppu); // LY 0 — window not drawn (WY=2)
        render_line(&mut ppu); // LY 1 — not drawn
        render_line(&mut ppu); // LY 2 — window line 0 (color 3)
        assert_eq!(pixel(&ppu, 0, 2), 3);
        render_line(&mut ppu); // LY 3 — window line 1 (color 1)
        assert_eq!(pixel(&ppu, 0, 3), 1);
    }

    #[test]
    fn sprite_draws_with_obp_and_transparency() {
        let mut ppu = Ppu::new();
        // Sprite tile: left half color 1, right half color 0 (transparent).
        let rows = [(0xF0u8, 0x00u8); 8]; // high nibble set → left 4 px color 1
        put_tile(&mut ppu, 1, rows);
        put_sprite(&mut ppu, 0, 16, 8, 1, 0); // screen (0,0), OBP0
        ppu.write_register(0xFF48, 0b11_10_01_00); // OBP0 identity
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_OBJ_ENABLE);
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 1); // left pixels drawn
        assert_eq!(pixel(&ppu, 4, 0), 0); // right pixels transparent → BG shade 0
    }

    #[test]
    fn sprite_x_and_y_flip() {
        let mut ppu = Ppu::new();
        // Row 0 has color 1 only in the leftmost pixel; other rows blank.
        let mut rows = [(0x00u8, 0x00u8); 8];
        rows[0] = (0x80, 0x00); // bit 7 → leftmost pixel color 1
        put_tile(&mut ppu, 1, rows);
        ppu.write_register(0xFF48, 0b11_10_01_00);
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_OBJ_ENABLE);

        // No flip: pixel appears at (0,0).
        put_sprite(&mut ppu, 0, 16, 8, 1, 0);
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 1);
        assert_eq!(pixel(&ppu, 7, 0), 0);

        // X flip: the leftmost source pixel lands at screen x 7.
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, rows);
        ppu.write_register(0xFF48, 0b11_10_01_00);
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_OBJ_ENABLE);
        put_sprite(&mut ppu, 0, 16, 8, 1, OBJ_ATTR_XFLIP);
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 7, 0), 1);
        assert_eq!(pixel(&ppu, 0, 0), 0);
    }

    #[test]
    fn sprite_obj_to_bg_priority() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(2)); // BG color 2
        put_tile(&mut ppu, 2, solid_tile(1)); // sprite color 1
        ppu.vram[0x1800] = 1; // BG map (0,0)
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF48, 0b11_10_01_00);
        put_sprite(&mut ppu, 0, 16, 8, 2, OBJ_ATTR_PRIORITY); // behind BG 1-3
        ppu.write_register(
            0xFF40,
            LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE | LCDC_OBJ_ENABLE,
        );
        render_line(&mut ppu);
        // BG color is 2 (nonzero) so the priority sprite stays hidden.
        assert_eq!(pixel(&ppu, 0, 0), 2);
    }

    #[test]
    fn sprite_over_bg_color_zero_even_with_priority() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 2, solid_tile(1)); // sprite color 1
        // BG left as all color 0 (no map/tile set up, BG enabled).
        ppu.write_register(0xFF47, 0b11_10_01_00);
        ppu.write_register(0xFF48, 0b11_10_01_00);
        put_sprite(&mut ppu, 0, 16, 8, 2, OBJ_ATTR_PRIORITY);
        ppu.write_register(
            0xFF40,
            LCDC_ENABLE | LCDC_TILE_DATA | LCDC_BG_ENABLE | LCDC_OBJ_ENABLE,
        );
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 1); // BG color 0 always loses
    }

    #[test]
    fn sprite_priority_smaller_x_wins() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(1));
        put_tile(&mut ppu, 2, solid_tile(2));
        ppu.write_register(0xFF48, 0b11_10_01_00);
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_OBJ_ENABLE);
        // Two overlapping sprites at the same X; lower OAM index (0) wins the tie
        // and is drawn last. Give them different X to test smaller-X-wins:
        put_sprite(&mut ppu, 0, 16, 10, 2, 0); // X larger (drawn first)
        put_sprite(&mut ppu, 1, 16, 8, 1, 0); // X smaller (higher priority)
        render_line(&mut ppu);
        // At screen x 2 both overlap; the smaller-X sprite (color 1) must show.
        assert_eq!(pixel(&ppu, 2, 0), 1);
    }

    #[test]
    fn ten_sprites_per_line_limit() {
        let mut ppu = Ppu::new();
        put_tile(&mut ppu, 1, solid_tile(1));
        ppu.write_register(0xFF48, 0b11_10_01_00);
        ppu.write_register(0xFF40, LCDC_ENABLE | LCDC_TILE_DATA | LCDC_OBJ_ENABLE);
        // 11 sprites on line 0, each 8 px wide, side by side. The 11th (X=88)
        // must not be drawn.
        for i in 0..11u8 {
            put_sprite(&mut ppu, i as usize, 16, 8 + i * 8, 1, 0);
        }
        render_line(&mut ppu);
        assert_eq!(pixel(&ppu, 0, 0), 1); // first sprite drawn
        assert_eq!(pixel(&ppu, 80, 0), 0); // 11th sprite's column not drawn
    }

    #[test]
    fn eight_by_sixteen_sprite_uses_two_tiles() {
        let mut ppu = Ppu::new();
        // Tile 2 = top half (color 1), tile 3 = bottom half (color 2). An 8x16
        // sprite with tile index 2 spans both.
        put_tile(&mut ppu, 2, solid_tile(1));
        put_tile(&mut ppu, 3, solid_tile(2));
        ppu.write_register(0xFF48, 0b11_10_01_00);
        ppu.write_register(
            0xFF40,
            LCDC_ENABLE | LCDC_TILE_DATA | LCDC_OBJ_ENABLE | LCDC_OBJ_SIZE,
        );
        put_sprite(&mut ppu, 0, 16, 8, 2, 0); // top-left at screen (0,0)
        render_line(&mut ppu); // LY 0 — top tile
        assert_eq!(pixel(&ppu, 0, 0), 1);
        for _ in 1..8 {
            render_line(&mut ppu);
        }
        render_line(&mut ppu); // LY 8 — bottom tile
        assert_eq!(pixel(&ppu, 0, 8), 2);
    }
}
