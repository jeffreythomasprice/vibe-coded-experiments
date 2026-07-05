//! The Game Boy (DMG) serial port (`SB`/`SC`, `0xFF01-0xFF02`).
//!
//! [`Serial`] is a **single-player stub**: it models just enough of the link
//! cable that a game which starts a transfer and then polls for it to finish (or
//! waits on the serial interrupt) doesn't hang. With no link partner connected,
//! real hardware clocks in `0xFF` for every bit, so a completed transfer leaves
//! `SB` = `0xFF`, clears the `SC` start bit, and raises the serial interrupt.
//!
//! Only the **internal clock** drives a transfer here: `SC` bit 0 = 1 selects the
//! internal 8192 Hz clock, so one byte takes `4194304 / 8192 * 8 = 4096` T-cycles.
//! A transfer started with the *external* clock (bit 0 = 0) waits for a clock that
//! a disconnected cable never supplies, so — like real hardware with no partner —
//! it simply never completes.
//!
//! Like the [`Timer`](crate::timer::Timer), the serial port knows nothing about
//! the interrupt registers: [`Serial::tick`] returns whether a transfer just
//! completed, and the owner ([`SystemBus`](crate::memory::SystemBus)) translates
//! that into a request on the `IF` serial bit. This mirrors the timer's
//! return-a-bool seam without a re-entrant borrow of the whole bus.
//!
//! The transmitted byte is **captured** as it is clocked out: each time a
//! transfer starts, the current `SB` (the byte the game just staged) is appended
//! to an output log that [`Serial::take_output`] drains. Nothing uses this in the
//! real machine yet, but it is what lets the headless test-ROM harness read the
//! ASCII stream Blargg's suites print to the link port. It is unrelated to the
//! `0xFF` clocked *in* from the disconnected cable on completion.
//!
//! Out of scope (later items): real link-cable emulation and the `[CGB]` fast
//! clock (`SC` bit 1) / double-speed timing.

/// `SB` (`0xFF01`), the 8-bit serial transfer data register.
const SB_ADDR: u16 = 0xFF01;
/// `SC` (`0xFF02`), the serial transfer control register.
const SC_ADDR: u16 = 0xFF02;

/// `SC` bit 7: transfer start / in-progress. Auto-clears on completion.
const SC_TRANSFER_START: u8 = 0x80;
/// `SC` bit 0: clock source (`1` = internal, `0` = external).
const SC_CLOCK_INTERNAL: u8 = 0x01;
/// The only `SC` bits writable on DMG: transfer-start and clock-source. (Bit 1,
/// the `[CGB]` fast clock, is ignored here.)
const SC_WRITABLE_MASK: u8 = SC_TRANSFER_START | SC_CLOCK_INTERNAL;
/// `SC` bits 6-1, unused on DMG and always read back as `1` (so an idle `SC`
/// reads the conventional post-boot `0x7E`).
const SC_UNUSED_MASK: u8 = 0x7E;

/// How long (in T-cycles) an internal-clock transfer of one byte takes:
/// 8 bits at the DMG's 8192 Hz internal clock, i.e. `4194304 / 8192 * 8`.
const TRANSFER_CYCLES: u32 = 4096;

/// The byte clocked in from a disconnected link cable.
const DISCONNECTED_BYTE: u8 = 0xFF;

/// The DMG serial port. See the module docs for the single-player stub model.
#[derive(Debug, Clone)]
pub struct Serial {
    /// `SB` (`0xFF01`), the transfer data register.
    sb: u8,
    /// `SC` (`0xFF02`); only the writable bits (start + clock-source) are stored.
    sc: u8,
    /// T-cycles remaining in an in-flight internal-clock transfer; `0` means no
    /// timed transfer is pending.
    remaining: u32,
    /// Bytes clocked *out* over the link port, in transmission order: one is
    /// appended each time a transfer starts (capturing the staged `SB`). Drained
    /// by [`Serial::take_output`]; see the module docs.
    output: Vec<u8>,
}

impl Default for Serial {
    fn default() -> Serial {
        Serial::new()
    }
}

impl Serial {
    /// A serial port in its power-on state: everything zeroed, matching how
    /// [`SystemBus`](crate::memory::SystemBus) powers on the rest of the machine
    /// zeroed (we skip the boot ROM). `read(SC)` then returns the conventional
    /// post-boot `0x7E`.
    pub fn new() -> Serial {
        Serial {
            sb: 0,
            sc: 0,
            remaining: 0,
            output: Vec::new(),
        }
    }

    /// Read one of the serial registers (`0xFF01-0xFF02`).
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            SB_ADDR => self.sb,
            SC_ADDR => self.sc | SC_UNUSED_MASK,
            _ => unreachable!("serial read of non-serial address {addr:#06x}"),
        }
    }

    /// Write one of the serial registers (`0xFF01-0xFF02`).
    ///
    /// Writing `SC` with the transfer-start bit and the internal clock both set
    /// kicks off a timed transfer; any other `SC` write (clearing start, or
    /// selecting the external clock) cancels a pending transfer.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            SB_ADDR => self.sb = value,
            SC_ADDR => {
                self.sc = value & SC_WRITABLE_MASK;
                let start_internal = SC_TRANSFER_START | SC_CLOCK_INTERNAL;
                if self.sc & start_internal == start_internal {
                    self.remaining = TRANSFER_CYCLES;
                    // The staged SB is the byte being clocked out; capture it now,
                    // before `complete()` overwrites SB with the disconnected 0xFF.
                    self.output.push(self.sb);
                } else {
                    self.remaining = 0;
                }
            }
            _ => unreachable!("serial write of non-serial address {addr:#06x}"),
        }
    }

    /// Advance an in-flight internal-clock transfer by `cycles` T-cycles,
    /// returning whether a transfer just completed (so the owner can request the
    /// serial interrupt). A completed transfer leaves `SB` = `0xFF` and clears
    /// the `SC` start bit.
    pub fn tick(&mut self, cycles: u32) -> bool {
        if self.remaining == 0 {
            return false;
        }
        if cycles >= self.remaining {
            self.remaining = 0;
            self.complete();
            true
        } else {
            self.remaining -= cycles;
            false
        }
    }

    fn complete(&mut self) {
        self.sb = DISCONNECTED_BYTE;
        self.sc &= !SC_TRANSFER_START;
        tracing::trace!("serial transfer complete: SB=0xFF, interrupt requested");
    }

    /// Drain the bytes clocked out over the link port since the last call, in
    /// transmission order. Empty when nothing has been transmitted. This is the
    /// seam the headless test-ROM harness reads Blargg's serial output through;
    /// the real desktop shell never calls it, so the log stays bounded there
    /// only if drained — otherwise it grows with each transmitted byte.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_reads_post_boot_sc() {
        let serial = Serial::new();
        assert_eq!(serial.read(SC_ADDR), 0x7E);
        assert_eq!(serial.read(SB_ADDR), 0x00);
    }

    #[test]
    fn sb_roundtrips() {
        let mut serial = Serial::new();
        serial.write(SB_ADDR, 0xA5);
        assert_eq!(serial.read(SB_ADDR), 0xA5);
    }

    #[test]
    fn sc_unused_bits_read_as_one() {
        let mut serial = Serial::new();
        serial.write(SC_ADDR, 0x00);
        assert_eq!(serial.read(SC_ADDR), 0x7E);
        // Bit 0 (clock select) is writable and reads back over the forced 1s.
        serial.write(SC_ADDR, 0x01);
        assert_eq!(serial.read(SC_ADDR), 0x7F);
        // An unused bit (e.g. bit 6) is not stored: it reads as 1 regardless.
        serial.write(SC_ADDR, 0x40);
        assert_eq!(serial.read(SC_ADDR), 0x7E);
    }

    #[test]
    fn internal_transfer_completes_after_4096_cycles() {
        let mut serial = Serial::new();
        serial.write(SB_ADDR, 0x42);
        serial.write(SC_ADDR, 0x81); // start + internal clock

        assert!(!serial.tick(4095));
        assert_eq!(serial.read(SC_ADDR) & SC_TRANSFER_START, SC_TRANSFER_START);

        assert!(serial.tick(1));
        assert_eq!(serial.read(SB_ADDR), 0xFF);
        assert_eq!(serial.read(SC_ADDR) & SC_TRANSFER_START, 0);
    }

    #[test]
    fn tick_span_larger_than_remaining_completes() {
        let mut serial = Serial::new();
        serial.write(SC_ADDR, 0x81);
        assert!(serial.tick(100_000));
        assert_eq!(serial.read(SB_ADDR), 0xFF);
        assert_eq!(serial.read(SC_ADDR) & SC_TRANSFER_START, 0);
    }

    #[test]
    fn external_clock_transfer_never_completes() {
        let mut serial = Serial::new();
        serial.write(SC_ADDR, 0x80); // start, but external clock (bit 0 = 0)
        assert!(!serial.tick(100_000));
        assert_eq!(serial.read(SC_ADDR) & SC_TRANSFER_START, SC_TRANSFER_START);
    }

    #[test]
    fn starting_a_transfer_captures_the_staged_sb_byte() {
        let mut serial = Serial::new();
        serial.write(SB_ADDR, b'H');
        serial.write(SC_ADDR, 0x81); // start + internal clock -> captures 'H'
        serial.tick(TRANSFER_CYCLES); // completion overwrites SB with 0xFF...

        serial.write(SB_ADDR, b'i');
        serial.write(SC_ADDR, 0x81); // ...but the second staged byte is still captured

        // External-clock and start-clearing writes transmit nothing.
        serial.write(SB_ADDR, b'?');
        serial.write(SC_ADDR, 0x80); // external clock: no capture
        serial.write(SC_ADDR, 0x01); // clears start: no capture

        assert_eq!(serial.take_output(), b"Hi");
        assert!(serial.take_output().is_empty(), "take_output drains the log");
    }

    #[test]
    fn no_transfer_no_interrupt() {
        let mut serial = Serial::new();
        assert!(!serial.tick(4096));
    }

    #[test]
    fn writing_sc_clearing_start_cancels_pending_transfer() {
        let mut serial = Serial::new();
        serial.write(SC_ADDR, 0x81); // start an internal-clock transfer
        serial.write(SC_ADDR, 0x01); // clear the start bit before it completes
        assert!(!serial.tick(4096));
        assert_eq!(serial.read(SC_ADDR) & SC_TRANSFER_START, 0);
    }
}
