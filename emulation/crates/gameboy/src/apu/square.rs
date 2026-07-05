//! The two square-wave channels (ch1 with a frequency sweep, ch2 without).
//!
//! Both are the same [`SquareChannel`] type; ch1 carries an `Option<Sweep>`
//! that ch2 leaves `None`. The channel exposes methods named by register
//! *function* (duty/length, envelope, frequency, control) rather than by
//! address, so [`super::Apu`] can route ch1's and ch2's differing addresses to
//! the same code.

use super::components::{dac_output, LengthCounter, Sweep, VolumeEnvelope};

/// The four duty-cycle waveforms (12.5%, 25%, 50%, 75%), one 8-sample period
/// each; `bit 7 = leftmost` in the register's mental model but here just an
/// 8-step ring.
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 0],
];

#[derive(Debug, Clone)]
pub(crate) struct SquareChannel {
    enabled: bool,
    duty: u8,
    duty_step: u8,
    frequency: u16,
    freq_timer: u16,
    length: LengthCounter,
    envelope: VolumeEnvelope,
    sweep: Option<Sweep>,
}

impl SquareChannel {
    pub(crate) fn new(has_sweep: bool) -> SquareChannel {
        SquareChannel {
            enabled: false,
            duty: 0,
            duty_step: 0,
            frequency: 0,
            freq_timer: 0,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::default(),
            sweep: if has_sweep { Some(Sweep::default()) } else { None },
        }
    }

    pub(crate) fn write_sweep(&mut self, value: u8) {
        if let Some(sweep) = &mut self.sweep {
            // Clearing the negate bit after a negate-mode calculation disables
            // the channel (the sweep-negate quirk).
            if sweep.negate_cleared_after_use(value) {
                self.enabled = false;
            }
            sweep.write(value);
        }
    }

    pub(crate) fn read_sweep(&self) -> u8 {
        self.sweep.as_ref().map_or(0xFF, |s| s.read() | 0x80)
    }

    pub(crate) fn write_length_duty(&mut self, value: u8) {
        self.duty = value >> 6;
        self.length.set_length(value & 0x3F);
    }

    pub(crate) fn read_length_duty(&self) -> u8 {
        (self.duty << 6) | 0x3F
    }

    pub(crate) fn write_envelope(&mut self, value: u8) {
        self.envelope.write(value);
        // Turning the DAC off disables the channel immediately.
        if !self.envelope.dac_enabled() {
            self.enabled = false;
        }
    }

    pub(crate) fn read_envelope(&self) -> u8 {
        self.envelope.read()
    }

    pub(crate) fn write_freq_lo(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x0700) | value as u16;
    }

    pub(crate) fn write_control(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x00FF) | (((value & 0x07) as u16) << 8);
        self.length.set_enabled(value & 0x40 != 0);
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    pub(crate) fn read_control(&self) -> u8 {
        ((self.length.is_enabled() as u8) << 6) | 0xBF
    }

    fn trigger(&mut self) {
        self.enabled = self.envelope.dac_enabled();
        self.freq_timer = self.period();
        self.length.trigger();
        self.envelope.trigger();
        if let Some(sweep) = &mut self.sweep {
            if sweep.trigger(self.frequency) {
                self.enabled = false;
            }
        }
    }

    /// T-cycles per duty step: `(2048 - frequency) * 4`.
    fn period(&self) -> u16 {
        (2048 - self.frequency) * 4
    }

    pub(crate) fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            self.freq_timer = self.period();
            self.duty_step = (self.duty_step + 1) & 7;
        }
    }

    pub(crate) fn clock_length(&mut self) {
        if self.length.tick() {
            self.enabled = false;
        }
    }

    pub(crate) fn clock_envelope(&mut self) {
        self.envelope.clock();
    }

    pub(crate) fn clock_sweep(&mut self) {
        if let Some(sweep) = &mut self.sweep {
            let update = sweep.clock();
            if let Some(freq) = update.new_frequency {
                self.frequency = freq;
            }
            if update.disable {
                self.enabled = false;
            }
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn digital(&self) -> u8 {
        if self.enabled {
            DUTY_TABLE[self.duty as usize][self.duty_step as usize] * self.envelope.volume()
        } else {
            0
        }
    }

    pub(crate) fn dac_output(&self) -> f32 {
        dac_output(self.digital(), self.envelope.dac_enabled())
    }

    pub(crate) fn reset(&mut self) {
        *self = SquareChannel::new(self.sweep.is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A triggered channel at max volume (no envelope stepping) and a given
    /// duty, so `digital()` is `0` or `15` per the duty waveform.
    fn triggered(duty: u8, frequency: u16) -> SquareChannel {
        let mut ch = SquareChannel::new(false);
        ch.write_envelope(0xF0); // volume 15, no envelope step, DAC on
        ch.write_length_duty(duty << 6);
        ch.write_freq_lo((frequency & 0xFF) as u8);
        ch.write_control(0x80 | ((frequency >> 8) as u8 & 0x07)); // trigger
        ch
    }

    #[test]
    fn duty_waveforms_match_the_table() {
        for duty in 0..4u8 {
            let mut ch = triggered(duty, 0);
            // duty_step starts at 0 and advances just before each output sample;
            // reproduce the 8-step ring and compare against the table.
            for step in 1..=8u8 {
                ch.tick_to_next_step();
                let expected = DUTY_TABLE[duty as usize][(step & 7) as usize] * 15;
                assert_eq!(ch.digital(), expected, "duty {duty} step {step}");
            }
        }
    }

    impl SquareChannel {
        /// Advance exactly one duty step (a whole frequency-timer period).
        fn tick_to_next_step(&mut self) {
            let before = self.duty_step;
            while self.duty_step == before {
                self.tick();
            }
        }
    }

    #[test]
    fn frequency_timer_reload_is_2048_minus_freq_times_four() {
        let mut ch = triggered(0, 2040); // period = (2048 - 2040) * 4 = 32
        let start = ch.duty_step;
        for _ in 0..31 {
            ch.tick();
        }
        assert_eq!(ch.duty_step, start, "no step before the period elapses");
        ch.tick();
        assert_eq!(ch.duty_step, (start + 1) & 7, "steps exactly at the period");
    }

    #[test]
    fn dac_off_forces_silence_and_blocks_trigger() {
        let mut ch = SquareChannel::new(false);
        ch.write_envelope(0x00); // DAC off
        ch.write_control(0x80); // attempt trigger
        assert!(!ch.is_enabled());
        assert_eq!(ch.dac_output(), 0.0);
    }

    #[test]
    fn length_expiry_disables_the_channel() {
        let mut ch = SquareChannel::new(false);
        ch.write_envelope(0xF0);
        ch.write_length_duty(63); // length load 63 -> counter 1
        ch.write_control(0xC0); // trigger + length enable
        assert!(ch.is_enabled());
        ch.clock_length();
        assert!(!ch.is_enabled(), "channel disabled when the length counter expires");
    }

    #[test]
    fn sweep_overflow_on_clock_disables_channel() {
        let mut ch = SquareChannel::new(true);
        ch.write_envelope(0xF0);
        ch.write_sweep((1 << 4) | 0x01); // period 1, additive, shift 1
        ch.write_freq_lo(0xE8);
        ch.write_control(0x80 | 0x03); // trigger, frequency = 0x3E8 = 1000
        // The trigger's immediate check (1000 + 500 = 1500) is in range.
        assert!(ch.is_enabled());
        // Sweep clock: 1500 written back, second check 1500 + 750 = 2250 > 2047.
        ch.clock_sweep();
        assert!(!ch.is_enabled());
    }

    #[test]
    fn sweep_updates_channel_frequency() {
        let mut ch = SquareChannel::new(true);
        ch.write_envelope(0xF0);
        ch.write_sweep((1 << 4) | 0x01); // period 1, additive, shift 1
        ch.write_freq_lo(0x00);
        ch.write_control(0x80 | 0x02); // trigger, frequency = 0x200 = 512
        ch.clock_sweep();
        assert_eq!(ch.frequency, 512 + 256);
    }

    #[test]
    fn reset_returns_to_power_on_state_preserving_sweep_presence() {
        let mut ch = SquareChannel::new(true);
        ch.write_envelope(0xF0);
        ch.write_control(0x80);
        ch.reset();
        assert!(!ch.is_enabled());
        assert!(ch.sweep.is_some());
    }
}
