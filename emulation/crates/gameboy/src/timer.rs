//! The Game Boy (DMG) timer and divider (`DIV`/`TIMA`/`TMA`/`TAC`,
//! `0xFF04-0xFF07`).
//!
//! [`Timer`] is modeled as a **falling-edge detector**, not a free-running
//! divider — the accurate hardware model. There is a single 16-bit internal
//! counter that increments once per T-cycle; `DIV` is simply its high byte. The
//! timer picks one bit of that counter (chosen by `TAC`'s clock-select bits),
//! ANDs it with the timer-enable bit, and increments `TIMA` on each **falling
//! edge** (1→0) of that combined signal.
//!
//! Modeling it this way makes the classic obscure behaviors fall out of one
//! place ([`Timer::detect_falling_edge`]) rather than being special-cased:
//! - Writing `DIV` resets the counter to 0, which can drop the selected bit
//!   from 1→0 and spuriously tick `TIMA`.
//! - Writing `TAC` (changing the selected bit or the enable) can likewise
//!   produce a falling edge and tick `TIMA`.
//!
//! `TIMA` overflow is **delayed one M-cycle** (4 T-cycles): for that window
//! `TIMA` reads `0x00`, and only afterwards is `TMA` loaded and the timer
//! interrupt requested. A write to `TIMA` during the window cancels the reload
//! and the interrupt.
//!
//! The timer knows nothing about the interrupt registers: [`Timer::tick`]
//! returns whether an overflow reload completed, and the owner
//! ([`SystemBus`](crate::memory::SystemBus)) translates that into a request on
//! the `IF` timer bit.
//! This mirrors the [`Cpu::request_interrupt`](crate::Cpu::request_interrupt)
//! seam without needing a re-entrant borrow of the whole bus. This timer is
//! unrelated to the MBC3 RTC.

/// The internal-counter bit whose falling edge advances `TIMA`, indexed by
/// `TAC`'s low two clock-select bits: `00`=÷1024 (4096 Hz), `01`=÷16
/// (262144 Hz), `10`=÷64 (65536 Hz), `11`=÷256 (16384 Hz).
const CLOCK_SELECT_BIT: [u32; 4] = [9, 3, 5, 7];

/// `TAC`'s three high bits that are unused and always read back as `1`.
const TAC_UNUSED_MASK: u8 = 0xF8;

/// `TAC` bit 2: the master timer-enable.
const TAC_ENABLE: u8 = 0b100;

/// How long (in T-cycles) `TIMA` stays `0x00` after overflow before `TMA` is
/// loaded and the interrupt is requested. One M-cycle on hardware.
const RELOAD_DELAY: u8 = 4;

/// State of the delayed `TIMA` overflow reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReloadState {
    /// No overflow pending; `TIMA` holds its normal value.
    Idle,
    /// `TIMA` overflowed and reads `0x00`; `TMA` is loaded (and the interrupt
    /// requested) once the given number of T-cycles have elapsed.
    Pending(u8),
}

/// The DMG timer/divider. See the module docs for the edge-detector model.
#[derive(Debug, Clone)]
pub struct Timer {
    /// The 16-bit internal counter. `DIV` (`0xFF04`) is its high byte.
    div_counter: u16,
    /// `TIMA` (`0xFF05`), the timer counter.
    tima: u8,
    /// `TMA` (`0xFF06`), the modulo loaded into `TIMA` on overflow.
    tma: u8,
    /// `TAC` (`0xFF07`); only the low three bits are significant.
    tac: u8,
    /// The previous value of the ANDed falling-edge input, so a 1→0 transition
    /// can be detected.
    prev_edge: bool,
    /// Delayed-overflow reload state.
    reload: ReloadState,
}

impl Default for Timer {
    fn default() -> Timer {
        Timer::new()
    }
}

impl Timer {
    /// A timer in its power-on state: everything zeroed, matching how
    /// [`SystemBus`](crate::memory::SystemBus) powers on the rest of the machine
    /// zeroed (we skip the boot ROM, so nothing seeds these registers).
    pub fn new() -> Timer {
        Timer {
            div_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            prev_edge: false,
            reload: ReloadState::Idle,
        }
    }

    /// Read one of the timer registers (`0xFF04-0xFF07`).
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.div_counter >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | TAC_UNUSED_MASK,
            _ => unreachable!("timer read of non-timer address {addr:#06x}"),
        }
    }

    /// Write one of the timer registers (`0xFF04-0xFF07`).
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Any write resets the whole internal counter. That can drop the
            // selected bit 1→0, so run the edge detector.
            0xFF04 => {
                self.div_counter = 0;
                self.detect_falling_edge();
            }
            // A write during the post-overflow delay window cancels the pending
            // reload and its interrupt; otherwise it just sets the counter.
            0xFF05 => {
                if matches!(self.reload, ReloadState::Pending(_)) {
                    self.reload = ReloadState::Idle;
                }
                self.tima = value;
            }
            0xFF06 => self.tma = value,
            // Changing the selected bit or the enable can produce a falling edge.
            0xFF07 => {
                self.tac = value & !TAC_UNUSED_MASK;
                self.detect_falling_edge();
            }
            _ => unreachable!("timer write of non-timer address {addr:#06x}"),
        }
    }

    /// Advance the timer by `cycles` T-cycles. Returns `true` if a `TIMA`
    /// overflow reload completed during this span (i.e. the caller should
    /// request the timer interrupt on `IF`).
    pub fn tick(&mut self, cycles: u32) -> bool {
        let mut interrupt = false;
        for _ in 0..cycles {
            interrupt |= self.tick_one();
        }
        interrupt
    }

    /// Advance a single T-cycle. Returns whether a delayed reload completed.
    fn tick_one(&mut self) -> bool {
        let mut interrupt = false;
        if let ReloadState::Pending(remaining) = self.reload {
            let remaining = remaining - 1;
            if remaining == 0 {
                self.tima = self.tma;
                self.reload = ReloadState::Idle;
                interrupt = true;
                tracing::trace!(tma = self.tma, "tima reloaded from tma; timer interrupt requested");
            } else {
                self.reload = ReloadState::Pending(remaining);
            }
        }
        self.div_counter = self.div_counter.wrapping_add(1);
        self.detect_falling_edge();
        interrupt
    }

    /// The current ANDed falling-edge input: the selected internal-counter bit
    /// gated by the timer-enable bit.
    fn edge_input(&self) -> bool {
        let enabled = self.tac & TAC_ENABLE != 0;
        let bit = CLOCK_SELECT_BIT[(self.tac & 0b11) as usize];
        enabled && (self.div_counter >> bit) & 1 != 0
    }

    /// Recompute the edge input and increment `TIMA` on a falling edge. Call
    /// after anything that can change the counter or `TAC`.
    fn detect_falling_edge(&mut self) {
        let input = self.edge_input();
        if self.prev_edge && !input {
            self.increment_tima();
        }
        self.prev_edge = input;
    }

    /// Increment `TIMA`, arming the delayed overflow reload on wrap.
    fn increment_tima(&mut self) {
        let (next, overflow) = self.tima.overflowing_add(1);
        self.tima = next;
        if overflow {
            // TIMA reads 0x00 during the delay; TMA is loaded RELOAD_DELAY
            // T-cycles later (see tick_one).
            self.tima = 0;
            self.reload = ReloadState::Pending(RELOAD_DELAY);
            tracing::trace!("tima overflow; reload pending");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A timer enabled with the given `TAC` clock-select value, counter at 0.
    fn enabled(clock_select: u8) -> Timer {
        let mut timer = Timer::new();
        timer.write(0xFF07, TAC_ENABLE | clock_select);
        timer
    }

    #[test]
    fn div_is_the_high_byte_of_the_internal_counter() {
        let mut timer = Timer::new();
        // 255 T-cycles is not enough to roll the high byte.
        timer.tick(0xFF);
        assert_eq!(timer.read(0xFF04), 0x00);
        // The 256th increment carries into the high byte.
        timer.tick(1);
        assert_eq!(timer.read(0xFF04), 0x01);
    }

    #[test]
    fn writing_div_resets_the_internal_counter() {
        let mut timer = Timer::new();
        timer.tick(0x0300); // high byte becomes 0x03
        assert_eq!(timer.read(0xFF04), 0x03);
        timer.write(0xFF04, 0xAB); // value is ignored; counter resets
        assert_eq!(timer.read(0xFF04), 0x00);
    }

    #[test]
    fn tima_increments_at_the_rate_selected_by_tac() {
        // period = number of T-cycles per TIMA increment for each clock select.
        for (clock_select, period) in [(0b00u8, 1024u32), (0b01, 16), (0b10, 64), (0b11, 256)] {
            let mut timer = enabled(clock_select);
            timer.tick(period - 1);
            assert_eq!(timer.read(0xFF05), 0, "clock select {clock_select:#04b} too early");
            timer.tick(1);
            assert_eq!(timer.read(0xFF05), 1, "clock select {clock_select:#04b} at period");
            timer.tick(period);
            assert_eq!(timer.read(0xFF05), 2, "clock select {clock_select:#04b} second tick");
        }
    }

    #[test]
    fn tima_does_not_increment_when_disabled() {
        let mut timer = Timer::new(); // TAC = 0: disabled, fastest select
        timer.tick(10_000);
        assert_eq!(timer.read(0xFF05), 0);
        // DIV still runs while the timer is disabled.
        assert_ne!(timer.read(0xFF04), 0);
    }

    #[test]
    fn tima_overflow_reload_is_delayed_then_requests_the_interrupt() {
        let mut timer = enabled(0b01); // ÷16
        timer.write(0xFF06, 0x7F); // TMA
        timer.write(0xFF05, 0xFF); // TIMA one short of overflow

        // 16 T-cycles produces the overflowing increment.
        let fired = timer.tick(16);
        assert!(!fired, "interrupt must not fire on the overflow cycle itself");
        assert_eq!(timer.read(0xFF05), 0x00, "TIMA reads 0x00 during the delay window");

        // It stays 0x00 for the first three of the four delay T-cycles.
        assert!(!timer.tick(3));
        assert_eq!(timer.read(0xFF05), 0x00);

        // The fourth T-cycle reloads TMA and requests the interrupt.
        let fired = timer.tick(1);
        assert!(fired);
        assert_eq!(timer.read(0xFF05), 0x7F);
    }

    #[test]
    fn writing_tima_during_the_delay_window_cancels_the_reload() {
        let mut timer = enabled(0b01);
        timer.write(0xFF06, 0x7F);
        timer.write(0xFF05, 0xFF);
        timer.tick(16); // overflow; reload pending, TIMA reads 0x00

        timer.write(0xFF05, 0x42); // cancel the reload
        let fired = timer.tick(RELOAD_DELAY as u32);
        assert!(!fired, "cancelled reload must not fire the interrupt");
        assert_eq!(timer.read(0xFF05), 0x42, "written value survives; TMA not loaded");
    }

    #[test]
    fn writing_div_can_spuriously_tick_tima() {
        // ÷16 selects bit 3. Advance to 8 T-cycles so bit 3 is 1 and enabled.
        let mut timer = enabled(0b01);
        timer.tick(8);
        assert_eq!(timer.read(0xFF05), 0);
        // Resetting the counter drops bit 3 from 1→0: a falling edge.
        timer.write(0xFF04, 0x00);
        assert_eq!(timer.read(0xFF05), 1);
    }

    #[test]
    fn changing_tac_can_spuriously_tick_tima() {
        // ÷16 selects bit 3; advance so it is 1 and enabled.
        let mut timer = enabled(0b01);
        timer.tick(8);
        assert_eq!(timer.read(0xFF05), 0);
        // Disabling the timer forces the ANDed input 1→0: a falling edge.
        timer.write(0xFF07, 0b000);
        assert_eq!(timer.read(0xFF05), 1);
    }

    #[test]
    fn tac_unused_upper_bits_read_as_one() {
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x00);
        assert_eq!(timer.read(0xFF07), 0xF8);
        timer.write(0xFF07, 0x05);
        assert_eq!(timer.read(0xFF07), 0xFD);
    }

    #[test]
    fn tma_round_trips() {
        let mut timer = Timer::new();
        timer.write(0xFF06, 0xCD);
        assert_eq!(timer.read(0xFF06), 0xCD);
    }
}
