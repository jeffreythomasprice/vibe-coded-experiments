//! The Game Boy (DMG) joypad register `P1/JOYP` (`0xFF00`).
//!
//! [`Joypad`] is a self-contained subsystem owned by
//! [`SystemBus`](crate::memory::SystemBus), mirroring the [`Timer`](crate::timer::Timer)
//! pattern: it owns all of its own state (the held-button bitfield and the last
//! written row-select bits) and knows nothing about the interrupt registers. Its
//! two mutators ([`Joypad::write`] and [`Joypad::set_pressed`]) return whether a
//! high→low transition on an input line occurred, and the owner translates that
//! into a request on the `IF` joypad bit — the same return-a-bool seam the timer
//! uses instead of a re-entrant borrow of the whole bus.
//!
//! The register models a 2×4 button matrix, all **active-low** (`0` = pressed /
//! selected):
//! - bits 7-6 are unused and read back as `1`;
//! - bit 5 (`P15`) `= 0` selects the **buttons** row (Start/Select/B/A);
//! - bit 4 (`P14`) `= 0` selects the **d-pad** row (Down/Up/Left/Right);
//! - bits 3-0 report the four inputs of the selected row(s): bit3 Down/Start,
//!   bit2 Up/Select, bit1 Left/B, bit0 Right/A, each `0` when held.
//!
//! Only the two select bits are writable; the input and unused bits ignore
//! writes. Everything powers on zeroed (the boot ROM is skipped), so both rows
//! start selected with nothing held and [`Joypad::read`] returns the conventional
//! post-boot value `0xCF`.
//!
//! The joypad interrupt (`INT $60`) fires on a high→low transition of any input
//! line — a press on a selected row, or a select change that newly pulls a held
//! line low. That edge is detected in one place ([`Joypad::detect_edge`]), the
//! same single-helper approach as the timer's falling-edge detector.

use std::collections::HashSet;

use crate::input::GameboyButton;

/// The two writable row-select bits of `P1` (bit 5 `P15` = buttons, bit 4 `P14`
/// = d-pad). Both are active-low: a `0` selects that row.
const SELECT_MASK: u8 = 0b0011_0000;

/// The unused high bits (7-6) of `P1`, which always read back as `1`.
const UNUSED_MASK: u8 = 0b1100_0000;

/// `P14` (bit 4): `0` selects the d-pad row.
const SELECT_DPAD: u8 = 0b0001_0000;

/// `P15` (bit 5): `0` selects the buttons row.
const SELECT_BUTTONS: u8 = 0b0010_0000;

/// The bit each button occupies in [`Joypad::pressed`]. The layout is chosen so
/// each hardware nibble is a direct slice of the field: the low nibble is the
/// d-pad row and the high nibble the buttons row, both already in the register's
/// bit3..0 order (Down/Up/Left/Right and Start/Select/B/A).
fn button_mask(button: GameboyButton) -> u8 {
    match button {
        GameboyButton::Right => 0x01,
        GameboyButton::Left => 0x02,
        GameboyButton::Up => 0x04,
        GameboyButton::Down => 0x08,
        GameboyButton::A => 0x10,
        GameboyButton::B => 0x20,
        GameboyButton::Select => 0x40,
        GameboyButton::Start => 0x80,
    }
}

/// The DMG joypad register. See the module docs for the matrix model.
#[derive(Debug, Clone)]
pub struct Joypad {
    /// Which buttons are currently held, as a bitfield (see [`button_mask`]); a
    /// `1` bit means held. The low nibble is the d-pad row, the high nibble the
    /// buttons row.
    pressed: u8,
    /// The last-written row-select bits (bits 4-5 of `P1`); only [`SELECT_MASK`]
    /// bits are significant. `0` in a bit selects that row (active-low).
    select: u8,
    /// The previously computed low nibble of the output lines, so a 1→0 edge on
    /// any input line can be detected across a mutation.
    prev_lines: u8,
}

impl Default for Joypad {
    fn default() -> Joypad {
        Joypad::new()
    }
}

impl Joypad {
    /// A joypad in its power-on state: nothing held and both rows selected
    /// (`select == 0`), matching how [`SystemBus`](crate::memory::SystemBus)
    /// powers on the rest of the machine zeroed. In that state [`Joypad::read`]
    /// returns `0xCF`.
    pub fn new() -> Joypad {
        Joypad {
            pressed: 0,
            select: 0,
            // With nothing held, all four lines are high — the initial edge
            // baseline.
            prev_lines: 0x0F,
        }
    }

    /// The active-low low nibble of `P1` given the current held buttons and row
    /// selection: a line reads `0` when a held button on a selected row drives
    /// it, `1` otherwise. If both rows are selected the two rows are ORed.
    fn lines(&self) -> u8 {
        let mut driven = 0;
        if self.select & SELECT_DPAD == 0 {
            driven |= self.pressed & 0x0F;
        }
        if self.select & SELECT_BUTTONS == 0 {
            driven |= self.pressed >> 4;
        }
        0x0F & !driven
    }

    /// Read `P1` (`0xFF00`): the forced-high unused bits, the row-select bits as
    /// last written, and the active-low input nibble for the selected row(s).
    pub fn read(&self) -> u8 {
        UNUSED_MASK | (self.select & SELECT_MASK) | self.lines()
    }

    /// Write `P1` (`0xFF00`): only the two row-select bits are stored; the input
    /// and unused bits are ignored. Returns whether the resulting select change
    /// pulled a held line high→low (i.e. the caller should request the joypad
    /// interrupt).
    pub fn write(&mut self, value: u8) -> bool {
        self.select = value & SELECT_MASK;
        self.detect_edge()
    }

    /// Replace the held-button set with `held`. Returns whether a newly-held
    /// button on a selected row produced a high→low edge on an input line (i.e.
    /// the caller should request the joypad interrupt).
    pub fn set_pressed(&mut self, held: &HashSet<GameboyButton>) -> bool {
        self.pressed = held.iter().fold(0, |acc, &b| acc | button_mask(b));
        self.detect_edge()
    }

    /// Recompute the output lines and report whether any of them fell 1→0 since
    /// the last mutation, updating the stored baseline. A 1→0 transition on an
    /// input line is exactly the joypad-interrupt condition.
    fn detect_edge(&mut self) -> bool {
        let new = self.lines();
        let edge = self.prev_lines & !new != 0;
        self.prev_lines = new;
        edge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(buttons: &[GameboyButton]) -> HashSet<GameboyButton> {
        buttons.iter().copied().collect()
    }

    #[test]
    fn powers_on_reading_cf() {
        assert_eq!(Joypad::new().read(), 0xCF);
    }

    #[test]
    fn unused_bits_always_high() {
        let mut pad = Joypad::new();
        // Even with a row selected and buttons held, bits 7-6 stay set.
        pad.write(SELECT_DPAD | SELECT_BUTTONS); // deselect both (bits 5-4 = 1)
        assert_eq!(pad.read() & UNUSED_MASK, UNUSED_MASK);
        pad.write(0x00); // select both
        pad.set_pressed(&held(&[GameboyButton::A, GameboyButton::Down]));
        assert_eq!(pad.read() & UNUSED_MASK, UNUSED_MASK);
    }

    #[test]
    fn only_select_bits_are_writable() {
        let mut pad = Joypad::new();
        // Set select to d-pad only (P14=0, P15=1); the low/unused bits in the
        // written byte must not stick.
        pad.write(!SELECT_DPAD); // all ones except P14
        assert_eq!(pad.read() & SELECT_MASK, SELECT_BUTTONS);
        // Nothing held, d-pad selected -> low nibble reads all high.
        assert_eq!(pad.read() & 0x0F, 0x0F);
    }

    #[test]
    fn dpad_row_reports_directions_active_low() {
        let mut pad = Joypad::new();
        pad.set_pressed(&held(&[GameboyButton::Right]));
        pad.write(SELECT_BUTTONS); // P15=1 (buttons off), P14=0 (d-pad on)
        // Right is bit0; active-low so it reads 0, the rest high.
        assert_eq!(pad.read(), UNUSED_MASK | SELECT_BUTTONS | 0b1110);
    }

    #[test]
    fn buttons_row_reports_buttons_active_low() {
        let mut pad = Joypad::new();
        pad.set_pressed(&held(&[GameboyButton::A]));
        pad.write(SELECT_DPAD); // P14=1 (d-pad off), P15=0 (buttons on)
        // A is bit0 of the buttons row; reads 0, the rest high.
        assert_eq!(pad.read(), UNUSED_MASK | SELECT_DPAD | 0b1110);
    }

    #[test]
    fn unselected_row_reads_high() {
        let mut pad = Joypad::new();
        pad.set_pressed(&held(&[GameboyButton::A])); // a buttons-row press
        pad.write(SELECT_BUTTONS); // select the d-pad row only
        // A is on the unselected row, so the low nibble is all high.
        assert_eq!(pad.read() & 0x0F, 0x0F);
    }

    #[test]
    fn both_rows_selected_unions_the_rows() {
        let mut pad = Joypad::new();
        // select == 0 (default) selects both rows. Down (d-pad bit3) and Start
        // (buttons bit3) both drive line 3; A drives line 0.
        pad.set_pressed(&held(&[GameboyButton::Down, GameboyButton::A]));
        assert_eq!(pad.read() & 0x0F, 0b1111 & !0b1001);
    }

    #[test]
    fn every_button_clears_its_expected_line() {
        // (button, selected row, expected line bit within the low nibble)
        let cases = [
            (GameboyButton::Right, SELECT_BUTTONS, 0b0001),
            (GameboyButton::Left, SELECT_BUTTONS, 0b0010),
            (GameboyButton::Up, SELECT_BUTTONS, 0b0100),
            (GameboyButton::Down, SELECT_BUTTONS, 0b1000),
            (GameboyButton::A, SELECT_DPAD, 0b0001),
            (GameboyButton::B, SELECT_DPAD, 0b0010),
            (GameboyButton::Select, SELECT_DPAD, 0b0100),
            (GameboyButton::Start, SELECT_DPAD, 0b1000),
        ];
        for (button, select, line) in cases {
            let mut pad = Joypad::new();
            pad.set_pressed(&held(&[button]));
            pad.write(select);
            assert_eq!(
                pad.read() & 0x0F,
                0x0F & !line,
                "{button:?} should clear line {line:#06b}",
            );
        }
    }

    #[test]
    fn pressing_selected_button_signals_edge() {
        let mut pad = Joypad::new();
        pad.write(SELECT_BUTTONS); // d-pad selected
        assert!(pad.set_pressed(&held(&[GameboyButton::Right])));
    }

    #[test]
    fn pressing_unselected_button_no_edge() {
        let mut pad = Joypad::new();
        pad.write(SELECT_BUTTONS); // d-pad selected, buttons row off
        assert!(!pad.set_pressed(&held(&[GameboyButton::A])));
    }

    #[test]
    fn releasing_button_no_edge() {
        let mut pad = Joypad::new();
        pad.write(SELECT_BUTTONS);
        assert!(pad.set_pressed(&held(&[GameboyButton::Right])));
        // Releasing is a low→high transition, not the interrupt condition.
        assert!(!pad.set_pressed(&held(&[])));
    }

    #[test]
    fn select_change_onto_held_button_signals_edge() {
        let mut pad = Joypad::new();
        // Hold A while the d-pad row is selected: A is invisible, no edge yet.
        pad.write(SELECT_BUTTONS);
        assert!(!pad.set_pressed(&held(&[GameboyButton::A])));
        // Now select the buttons row: A's line falls 1→0, an edge.
        assert!(pad.write(SELECT_DPAD));
    }
}
