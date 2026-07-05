//! MBC3 real-time clock.
//!
//! The RTC exposes five registers — seconds, minutes, hours, and a 9-bit day
//! counter split across a low byte and a control byte (day bit 8 + halt + carry).
//! We back it with a live wall clock: the `base` registers plus an `anchor`
//! instant record "the clock read *this* at *that* moment", and we compute the
//! current time by adding the elapsed real seconds when the game latches. This
//! is cheap (games latch rarely, only when displaying time) and avoids touching
//! the wall clock on the hot path.
//!
//! The clock persists across sessions via [`Rtc::to_bytes`] / [`Rtc::from_bytes`]:
//! we serialize the `base`/`latched` registers plus the `anchor` as a wall-clock
//! timestamp. Because the anchor is absolute time (not "seconds since load"), a
//! restored clock keeps ticking through the interval the emulator was closed —
//! matching a real cartridge whose battery-backed RTC never stops.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// RTC register selectors as written to `0x4000-0x5FFF`.
pub(super) const REG_SECONDS: u8 = 0x08;
pub(super) const REG_MINUTES: u8 = 0x09;
pub(super) const REG_HOURS: u8 = 0x0A;
pub(super) const REG_DAY_LOW: u8 = 0x0B;
pub(super) const REG_DAY_HIGH: u8 = 0x0C;

const DAY_HIGH_BIT: u16 = 0x0100;
const HALT_FLAG: u8 = 0x40;
const CARRY_FLAG: u8 = 0x80;

/// Serialized size of one [`RtcRegs`] snapshot: S, M, H, `days:u16`, flags.
const REGS_LEN: usize = 6;
/// Serialized size of the RTC save trailer: `base` regs + `latched` regs + an
/// 8-byte wall-clock anchor.
pub(in crate::cartridge) const RTC_SAVE_LEN: usize = REGS_LEN * 2 + 8;

impl RtcRegs {
    /// Pack into [`REGS_LEN`] little-endian bytes.
    fn to_bytes(self) -> [u8; REGS_LEN] {
        let [day_lo, day_hi] = self.days.to_le_bytes();
        let mut flags = 0u8;
        if self.halt {
            flags |= 0x01;
        }
        if self.carry {
            flags |= 0x02;
        }
        [self.seconds, self.minutes, self.hours, day_lo, day_hi, flags]
    }

    /// Unpack a [`REGS_LEN`]-byte slice written by [`RtcRegs::to_bytes`].
    fn from_bytes(b: &[u8]) -> RtcRegs {
        RtcRegs {
            seconds: b[0],
            minutes: b[1],
            hours: b[2],
            days: u16::from_le_bytes([b[3], b[4]]),
            halt: b[5] & 0x01 != 0,
            carry: b[5] & 0x02 != 0,
        }
    }
}

/// A snapshot of the five clock counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RtcRegs {
    seconds: u8,
    minutes: u8,
    hours: u8,
    /// 9-bit day counter (0..=511).
    days: u16,
    halt: bool,
    /// Day-counter carry: sticks until the program clears it.
    carry: bool,
}

/// The live wall-clock RTC.
#[derive(Debug)]
pub(super) struct Rtc {
    base: RtcRegs,
    anchor: SystemTime,
    latched: RtcRegs,
    /// The previous value written to the latch register, to detect `0 -> 1`.
    last_latch_write: Option<u8>,
}

impl Rtc {
    pub(super) fn new() -> Rtc {
        Rtc {
            base: RtcRegs::default(),
            anchor: SystemTime::now(),
            latched: RtcRegs::default(),
            last_latch_write: None,
        }
    }

    /// Serialize the clock for the save file: `base` regs, `latched` regs, and
    /// the `anchor` as seconds since the Unix epoch. `last_latch_write` is
    /// transient (it only tracks a two-step latch sequence in progress) and is
    /// not persisted.
    pub(in crate::cartridge) fn to_bytes(&self) -> Vec<u8> {
        let anchor_secs = self
            .anchor
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let mut out = Vec::with_capacity(RTC_SAVE_LEN);
        out.extend_from_slice(&self.base.to_bytes());
        out.extend_from_slice(&self.latched.to_bytes());
        out.extend_from_slice(&anchor_secs.to_le_bytes());
        out
    }

    /// Restore a clock previously serialized by [`Rtc::to_bytes`]. Returns `None`
    /// if the slice is too short to hold the trailer.
    pub(in crate::cartridge) fn from_bytes(data: &[u8]) -> Option<Rtc> {
        if data.len() < RTC_SAVE_LEN {
            return None;
        }
        let base = RtcRegs::from_bytes(&data[0..REGS_LEN]);
        let latched = RtcRegs::from_bytes(&data[REGS_LEN..REGS_LEN * 2]);
        let anchor_secs = u64::from_le_bytes(
            data[REGS_LEN * 2..REGS_LEN * 2 + 8]
                .try_into()
                .expect("slice is exactly 8 bytes"),
        );
        Some(Rtc {
            base,
            anchor: UNIX_EPOCH + Duration::from_secs(anchor_secs),
            latched,
            last_latch_write: None,
        })
    }

    /// Handle a write to the latch register (`0x6000-0x7FFF`): a `0x00` then
    /// `0x01` sequence copies the live clock into the latched registers.
    pub(super) fn write_latch(&mut self, value: u8) {
        self.write_latch_at(value, SystemTime::now());
    }

    fn write_latch_at(&mut self, value: u8, now: SystemTime) {
        if self.last_latch_write == Some(0x00) && value == 0x01 {
            self.latched = self.live(now);
        }
        self.last_latch_write = Some(value);
    }

    /// Read a latched RTC register by its `0x08-0x0C` selector.
    pub(super) fn read(&self, reg: u8) -> u8 {
        let r = &self.latched;
        match reg {
            REG_SECONDS => r.seconds,
            REG_MINUTES => r.minutes,
            REG_HOURS => r.hours,
            REG_DAY_LOW => (r.days & 0x00FF) as u8,
            REG_DAY_HIGH => {
                let mut v = ((r.days & DAY_HIGH_BIT) >> 8) as u8;
                if r.halt {
                    v |= HALT_FLAG;
                }
                if r.carry {
                    v |= CARRY_FLAG;
                }
                v
            }
            _ => 0xFF,
        }
    }

    /// Write an RTC register. Writing sets the clock: we snapshot the current
    /// live value, apply the field, and re-anchor to now so the clock keeps
    /// running from the written value.
    pub(super) fn write(&mut self, reg: u8, value: u8) {
        self.write_at(reg, value, SystemTime::now());
    }

    fn write_at(&mut self, reg: u8, value: u8, now: SystemTime) {
        let mut regs = self.live(now);
        match reg {
            REG_SECONDS => regs.seconds = value & 0x3F,
            REG_MINUTES => regs.minutes = value & 0x3F,
            REG_HOURS => regs.hours = value & 0x1F,
            REG_DAY_LOW => regs.days = (regs.days & DAY_HIGH_BIT) | value as u16,
            REG_DAY_HIGH => {
                regs.days = (regs.days & 0x00FF) | ((value as u16 & 0x01) << 8);
                regs.halt = value & HALT_FLAG != 0;
                regs.carry = value & CARRY_FLAG != 0;
            }
            _ => return,
        }
        self.base = regs;
        self.anchor = now;
    }

    /// The live register values at `now`, advancing `base` by the elapsed real
    /// seconds since the anchor (unless halted).
    fn live(&self, now: SystemTime) -> RtcRegs {
        if self.base.halt {
            return self.base;
        }
        // Clamp non-monotonic clocks (NTP step, DST) to a non-negative delta.
        let elapsed = now
            .duration_since(self.anchor)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        advance(self.base, elapsed)
    }
}

/// Advance a register snapshot by `add_secs`, propagating carries into the
/// 9-bit day counter and setting the sticky day-carry flag on overflow.
fn advance(mut r: RtcRegs, add_secs: u64) -> RtcRegs {
    let mut total = r.seconds as u64
        + r.minutes as u64 * 60
        + r.hours as u64 * 3600
        + r.days as u64 * 86_400
        + add_secs;

    r.seconds = (total % 60) as u8;
    total /= 60;
    r.minutes = (total % 60) as u8;
    total /= 60;
    r.hours = (total % 24) as u8;
    total /= 24;
    // `total` is now the whole day count.
    if total > 0x01FF {
        r.carry = true;
    }
    r.days = (total % 0x0200) as u16;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_propagates_carries() {
        let r = advance(RtcRegs::default(), 3661); // 1h 1m 1s
        assert_eq!((r.hours, r.minutes, r.seconds), (1, 1, 1));
        assert_eq!(r.days, 0);
    }

    #[test]
    fn advance_counts_days_and_sets_carry() {
        let r = advance(RtcRegs::default(), 512 * 86_400); // 512 days -> overflow
        assert_eq!(r.days, 0);
        assert!(r.carry);
    }

    #[test]
    fn latch_copies_live_at_elapsed_time() {
        let mut rtc = Rtc::new();
        let start = rtc.anchor;
        // Latch after a simulated 90 seconds.
        let now = start + Duration::from_secs(90);
        rtc.write_latch_at(0x00, now);
        rtc.write_latch_at(0x01, now);
        assert_eq!(rtc.read(REG_MINUTES), 1);
        assert_eq!(rtc.read(REG_SECONDS), 30);
    }

    #[test]
    fn latch_requires_zero_then_one() {
        let mut rtc = Rtc::new();
        let now = rtc.anchor + Duration::from_secs(5);
        // A lone 0x01 with no preceding 0x00 must not latch.
        rtc.write_latch_at(0x01, now);
        assert_eq!(rtc.read(REG_SECONDS), 0);
    }

    #[test]
    fn writing_registers_sets_and_reanchors() {
        let mut rtc = Rtc::new();
        let t0 = rtc.anchor;
        rtc.write_at(REG_HOURS, 10, t0);
        // 30 seconds later, latch: hours preserved, seconds advanced.
        let t1 = t0 + Duration::from_secs(30);
        rtc.write_latch_at(0x00, t1);
        rtc.write_latch_at(0x01, t1);
        assert_eq!(rtc.read(REG_HOURS), 10);
        assert_eq!(rtc.read(REG_SECONDS), 30);
    }

    #[test]
    fn halt_freezes_the_clock() {
        let mut rtc = Rtc::new();
        let t0 = rtc.anchor;
        rtc.write_at(REG_DAY_HIGH, HALT_FLAG, t0); // set halt
        let t1 = t0 + Duration::from_secs(120);
        rtc.write_latch_at(0x00, t1);
        rtc.write_latch_at(0x01, t1);
        assert_eq!(rtc.read(REG_SECONDS), 0); // did not advance
    }

    #[test]
    fn latched_registers_round_trip_through_bytes() {
        let mut rtc = Rtc::new();
        let t0 = UNIX_EPOCH + Duration::from_secs(1_000_000);
        rtc.write_at(REG_HOURS, 5, t0);
        rtc.write_at(REG_MINUTES, 30, t0);
        rtc.write_latch_at(0x00, t0);
        rtc.write_latch_at(0x01, t0); // latch h=5, m=30

        let bytes = rtc.to_bytes();
        assert_eq!(bytes.len(), RTC_SAVE_LEN);
        let restored = Rtc::from_bytes(&bytes).unwrap();
        assert_eq!(restored.read(REG_HOURS), 5);
        assert_eq!(restored.read(REG_MINUTES), 30);
    }

    #[test]
    fn restored_clock_advances_across_downtime() {
        // Anchoring to absolute time means the clock keeps running through the
        // interval the emulator was closed, like a real battery-backed RTC.
        let mut rtc = Rtc::new();
        let t0 = UNIX_EPOCH + Duration::from_secs(1_000_000);
        rtc.write_at(REG_SECONDS, 0, t0); // base zero, anchored at t0
        let restored = Rtc::from_bytes(&rtc.to_bytes()).unwrap();
        let live = restored.live(t0 + Duration::from_secs(100));
        assert_eq!((live.minutes, live.seconds), (1, 40));
    }

    #[test]
    fn halt_flag_round_trips_and_freezes() {
        let mut rtc = Rtc::new();
        let t0 = UNIX_EPOCH + Duration::from_secs(1_000_000);
        rtc.write_at(REG_DAY_HIGH, HALT_FLAG, t0);
        let restored = Rtc::from_bytes(&rtc.to_bytes()).unwrap();
        assert!(restored.base.halt);
        let live = restored.live(t0 + Duration::from_secs(500));
        assert_eq!(live.seconds, 0); // halted: no advance even after restore
    }

    #[test]
    fn from_bytes_rejects_short_input() {
        assert!(Rtc::from_bytes(&[0u8; RTC_SAVE_LEN - 1]).is_none());
    }
}
