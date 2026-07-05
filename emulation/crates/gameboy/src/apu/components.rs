//! Shared DSP building blocks used by more than one APU channel: the length
//! counter, the volume envelope, and channel 1's frequency sweep.
//!
//! Each is a small self-contained state machine clocked by the frame sequencer
//! (see [`super::frame_sequencer`]). Keeping them here — rather than duplicated
//! inside each channel — is what lets the square/wave/noise channels stay thin
//! and lets these fiddly bits (envelope stepping, the sweep overflow/negate
//! rules) be unit-tested in isolation.

/// Map a channel's 4-bit digital output (`0-15`) to an analog sample in
/// `[-1.0, 1.0]`. A powered-off DAC produces analog silence (`0.0`) regardless
/// of the digital value; a powered DAC producing digital `0` sits at the `-1.0`
/// rail (this is the DC level a disabled-but-DAC-on channel holds on hardware).
pub(crate) fn dac_output(digital: u8, dac_on: bool) -> f32 {
    if dac_on {
        digital as f32 / 7.5 - 1.0
    } else {
        0.0
    }
}

/// A channel's length counter: counts down at 256 Hz and disables the channel
/// when it reaches zero, but only while length-enable (`NRx4` bit 6) is set.
///
/// The reload maximum differs by channel: 64 for the square and noise channels
/// (6-bit length load) and 256 for the wave channel (8-bit load).
#[derive(Debug, Clone)]
pub(crate) struct LengthCounter {
    counter: u16,
    enabled: bool,
    max: u16,
}

impl LengthCounter {
    pub(crate) fn new(max: u16) -> LengthCounter {
        LengthCounter {
            counter: 0,
            enabled: false,
            max,
        }
    }

    /// Load the length from a register write (`NRx1`/`NR31`). The stored counter
    /// is `max - load`, so a larger load means a shorter time to expiry.
    pub(crate) fn set_length(&mut self, load: u8) {
        self.counter = self.max - load as u16;
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Advance one 256 Hz length clock. Returns `true` if the counter just
    /// reached zero (the owning channel should disable itself).
    pub(crate) fn tick(&mut self) -> bool {
        if !self.enabled || self.counter == 0 {
            return false;
        }
        self.counter -= 1;
        self.counter == 0
    }

    /// On channel trigger, a zero counter reloads to the maximum.
    pub(crate) fn trigger(&mut self) {
        if self.counter == 0 {
            self.counter = self.max;
        }
    }
}

/// A channel's volume envelope: steps the output volume up or down at 64 Hz.
#[derive(Debug, Clone, Default)]
pub(crate) struct VolumeEnvelope {
    /// The volume loaded on trigger (`NRx2` bits 4-7).
    initial_volume: u8,
    /// Direction: `true` steps up (louder), `false` steps down (`NRx2` bit 3).
    add_mode: bool,
    /// Step period in 64 Hz ticks; `0` disables stepping (`NRx2` bits 0-2).
    period: u8,
    /// The live volume (0-15).
    volume: u8,
    /// Countdown to the next step.
    timer: u8,
}

impl VolumeEnvelope {
    /// Decode an `NRx2` write. Does not change the live volume — that only
    /// happens on trigger.
    pub(crate) fn write(&mut self, nrx2: u8) {
        self.initial_volume = nrx2 >> 4;
        self.add_mode = nrx2 & 0x08 != 0;
        self.period = nrx2 & 0x07;
    }

    /// Reconstruct the `NRx2` register byte for reads.
    pub(crate) fn read(&self) -> u8 {
        (self.initial_volume << 4) | ((self.add_mode as u8) << 3) | self.period
    }

    /// The DAC is powered whenever any of `NRx2`'s top five bits are set; a
    /// channel whose DAC is off produces no output and cannot be triggered on.
    pub(crate) fn dac_enabled(&self) -> bool {
        self.initial_volume != 0 || self.add_mode
    }

    pub(crate) fn trigger(&mut self) {
        self.volume = self.initial_volume;
        self.timer = self.period;
    }

    /// Advance one 64 Hz envelope clock.
    pub(crate) fn clock(&mut self) {
        if self.period == 0 {
            return;
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer == 0 {
            self.timer = self.period;
            if self.add_mode && self.volume < 15 {
                self.volume += 1;
            } else if !self.add_mode && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }

    pub(crate) fn volume(&self) -> u8 {
        self.volume
    }
}

/// The result of one 128 Hz sweep clock: an optional new frequency to write back
/// into the channel, and whether a frequency overflow just disabled the channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SweepUpdate {
    pub(crate) new_frequency: Option<u16>,
    pub(crate) disable: bool,
}

/// Channel 1's frequency sweep (`NR10`): periodically shifts the channel
/// frequency up or down, disabling the channel if a computed frequency would
/// overflow the 11-bit range.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sweep {
    period: u8,
    negate: bool,
    shift: u8,
    timer: u8,
    enabled: bool,
    shadow: u16,
    /// Set once a calculation has run in negate mode. Clearing the negate bit
    /// afterwards disables the channel (the "sweep negate" quirk).
    negate_used: bool,
}

impl Sweep {
    pub(crate) fn write(&mut self, nr10: u8) {
        self.period = (nr10 >> 4) & 0x07;
        self.negate = nr10 & 0x08 != 0;
        self.shift = nr10 & 0x07;
    }

    /// Reconstruct the `NR10` register byte for reads.
    pub(crate) fn read(&self) -> u8 {
        (self.period << 4) | ((self.negate as u8) << 3) | self.shift
    }

    /// Whether an `NR10` write clears the negate bit after a negate-mode
    /// calculation has already happened — which disables the channel.
    pub(crate) fn negate_cleared_after_use(&self, nr10: u8) -> bool {
        self.negate_used && nr10 & 0x08 == 0
    }

    /// Trigger: latch the current frequency, reset the timer, enable the sweep
    /// if it has a period or shift, and run one immediate overflow check when a
    /// shift is configured. Returns whether that check overflowed (which
    /// disables the channel).
    pub(crate) fn trigger(&mut self, frequency: u16) -> bool {
        self.shadow = frequency;
        self.timer = if self.period != 0 { self.period } else { 8 };
        self.enabled = self.period != 0 || self.shift != 0;
        self.negate_used = false;
        if self.shift != 0 {
            self.calc() > 2047
        } else {
            false
        }
    }

    /// Advance one 128 Hz sweep clock.
    pub(crate) fn clock(&mut self) -> SweepUpdate {
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer != 0 {
            return SweepUpdate::default();
        }
        self.timer = if self.period != 0 { self.period } else { 8 };
        if !self.enabled || self.period == 0 {
            return SweepUpdate::default();
        }
        let new = self.calc();
        if new > 2047 {
            return SweepUpdate {
                new_frequency: None,
                disable: true,
            };
        }
        if self.shift != 0 {
            self.shadow = new;
            // A second calculation is performed and its overflow (only) checked;
            // the result is not written back.
            let disable = self.calc() > 2047;
            SweepUpdate {
                new_frequency: Some(new),
                disable,
            }
        } else {
            SweepUpdate::default()
        }
    }

    /// `shadow ± (shadow >> shift)`. The subtraction never underflows because
    /// `shadow >> shift <= shadow`; the addition can exceed 2047 (the overflow
    /// the callers check for).
    fn calc(&mut self) -> u16 {
        let delta = self.shadow >> self.shift;
        if self.negate {
            self.negate_used = true;
            self.shadow - delta
        } else {
            self.shadow + delta
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dac_output_maps_digital_to_analog() {
        assert_eq!(dac_output(0, true), -1.0);
        assert_eq!(dac_output(15, true), 1.0);
        // A powered-off DAC is silence regardless of the digital value.
        assert_eq!(dac_output(15, false), 0.0);
        assert_eq!(dac_output(0, false), 0.0);
    }

    #[test]
    fn length_counter_reloads_max_minus_load() {
        // Square/noise use a 64-step counter; wave uses 256.
        for (max, load, remaining) in [(64u16, 0u8, 64u16), (64, 63, 1), (256, 0, 256), (256, 255, 1)]
        {
            let mut length = LengthCounter::new(max);
            length.set_length(load);
            length.set_enabled(true);
            for _ in 0..remaining - 1 {
                assert!(!length.tick(), "max {max} load {load} expired early");
            }
            assert!(length.tick(), "max {max} load {load} should expire on the last tick");
        }
    }

    #[test]
    fn length_counter_only_ticks_when_enabled() {
        let mut length = LengthCounter::new(64);
        length.set_length(63); // counter = 1
        // Disabled: it never counts down or expires.
        assert!(!length.tick());
        length.set_enabled(true);
        assert!(length.tick());
    }

    #[test]
    fn length_counter_trigger_reloads_only_when_zero() {
        let mut length = LengthCounter::new(64);
        length.set_enabled(true);
        length.set_length(63); // counter = 1
        assert!(length.tick()); // -> 0
        length.trigger(); // reloads to 64
        for _ in 0..63 {
            assert!(!length.tick());
        }
        assert!(length.tick());
    }

    #[test]
    fn envelope_decodes_nrx2() {
        let mut env = VolumeEnvelope::default();
        env.write(0xF3); // volume 15, add, period 3
        assert_eq!(env.read(), 0xF3);
        assert!(env.dac_enabled());
        env.trigger();
        assert_eq!(env.volume(), 15);
    }

    #[test]
    fn envelope_dac_off_only_when_top_five_bits_zero() {
        let mut env = VolumeEnvelope::default();
        env.write(0x00);
        assert!(!env.dac_enabled());
        // The direction bit alone keeps the DAC on even at volume 0.
        env.write(0x08);
        assert!(env.dac_enabled());
        env.write(0x10);
        assert!(env.dac_enabled());
    }

    #[test]
    fn envelope_steps_toward_bounds_and_saturates() {
        // Increasing envelope, period 1: +1 per clock, saturating at 15.
        let mut env = VolumeEnvelope::default();
        env.write((5 << 4) | 0x08 | 1);
        env.trigger();
        assert_eq!(env.volume(), 5);
        for expected in 6..=15 {
            env.clock();
            assert_eq!(env.volume(), expected);
        }
        env.clock();
        assert_eq!(env.volume(), 15, "saturates at 15");

        // Decreasing envelope, period 2: -1 every two clocks, saturating at 0.
        let mut env = VolumeEnvelope::default();
        env.write((2 << 4) | 2);
        env.trigger();
        env.clock();
        assert_eq!(env.volume(), 2, "no step until the period elapses");
        env.clock();
        assert_eq!(env.volume(), 1);
        env.clock();
        env.clock();
        assert_eq!(env.volume(), 0);
    }

    #[test]
    fn envelope_period_zero_never_steps() {
        let mut env = VolumeEnvelope::default();
        env.write((8 << 4) | 0x08); // period 0
        env.trigger();
        for _ in 0..10 {
            env.clock();
        }
        assert_eq!(env.volume(), 8);
    }

    #[test]
    fn sweep_calc_adds_or_subtracts_shifted_frequency() {
        for (freq, shift, negate, expected) in
            [(1024u16, 1u8, false, 1536u16), (1024, 2, false, 1280), (1024, 1, true, 512)]
        {
            let mut sweep = Sweep::default();
            sweep.write(((negate as u8) << 3) | shift); // sweep period 0
            sweep.trigger(freq);
            assert_eq!(sweep.calc(), expected, "freq {freq} shift {shift} negate {negate}");
        }
    }

    #[test]
    fn sweep_trigger_with_shift_detects_immediate_overflow() {
        let mut sweep = Sweep::default();
        sweep.write(0x01); // shift 1, additive
        // 2000 + (2000 >> 1) = 3000 > 2047 -> overflow disables on trigger.
        assert!(sweep.trigger(2000));
        // A frequency that does not overflow keeps the channel alive.
        let mut sweep = Sweep::default();
        sweep.write(0x01);
        assert!(!sweep.trigger(1000));
    }

    #[test]
    fn sweep_clock_updates_frequency_then_overflows() {
        let mut sweep = Sweep::default();
        sweep.write((1 << 4) | 0x02); // period 1, additive, shift 2
        sweep.trigger(1300);
        // Clock 1: 1300 + 325 = 1625 written back; the second check (1625 + 406 =
        // 2031) stays in range, so no disable.
        let update = sweep.clock();
        assert_eq!(update.new_frequency, Some(1625));
        assert!(!update.disable);
        // Clock 2: 2031 written back, but the second check (2031 + 507 = 2538)
        // overflows — the frequency still updates and the channel is disabled.
        let update = sweep.clock();
        assert_eq!(update.new_frequency, Some(2031));
        assert!(update.disable);
        // Clock 3: the first calculation itself (2031 + 507) overflows, so
        // nothing is written back.
        let update = sweep.clock();
        assert_eq!(update.new_frequency, None);
        assert!(update.disable);
    }

    #[test]
    fn sweep_period_zero_does_not_update_frequency() {
        let mut sweep = Sweep::default();
        sweep.write(0x01); // period 0, shift 1
        sweep.trigger(1000);
        // Period 0: the timer cycles (reloaded to 8) but no calculation runs.
        for _ in 0..20 {
            assert_eq!(sweep.clock(), SweepUpdate::default());
        }
    }

    #[test]
    fn sweep_negate_cleared_after_use_flags_disable() {
        let mut sweep = Sweep::default();
        sweep.write(0x09); // negate, shift 1
        sweep.trigger(1000); // immediate calc runs in negate mode -> negate_used
        assert!(sweep.negate_cleared_after_use(0x01)); // clearing negate now disables
        assert!(!sweep.negate_cleared_after_use(0x09)); // keeping negate is fine
    }
}
