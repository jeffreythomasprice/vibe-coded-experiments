//! Channel 4: the noise channel. A linear-feedback shift register (LFSR)
//! clocked at a programmable rate produces pseudo-random 1-bit noise, gated by a
//! volume envelope. `NR43` bit 3 switches the LFSR between 15-bit and 7-bit
//! width, which changes the timbre from hiss to a buzzier periodic tone.

use super::components::{dac_output, LengthCounter, VolumeEnvelope};

/// Base divisor selected by `NR43`'s low three bits; the actual period is this
/// shifted left by the clock-shift field. (Code 0 is a half-step, hence 8.)
const DIVISORS: [u16; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

#[derive(Debug, Clone)]
pub(crate) struct NoiseChannel {
    enabled: bool,
    length: LengthCounter,
    envelope: VolumeEnvelope,
    clock_shift: u8,
    width_7bit: bool,
    divisor_code: u8,
    /// The divisor period can exceed 16 bits at large clock shifts, so it is a
    /// `u32` (unlike the square/wave frequency timers).
    freq_timer: u32,
    lfsr: u16,
}

impl NoiseChannel {
    pub(crate) fn new() -> NoiseChannel {
        NoiseChannel {
            enabled: false,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::default(),
            clock_shift: 0,
            width_7bit: false,
            divisor_code: 0,
            freq_timer: 0,
            lfsr: 0x7FFF,
        }
    }

    pub(crate) fn write_length(&mut self, value: u8) {
        self.length.set_length(value & 0x3F);
    }

    pub(crate) fn write_envelope(&mut self, value: u8) {
        self.envelope.write(value);
        if !self.envelope.dac_enabled() {
            self.enabled = false;
        }
    }

    pub(crate) fn read_envelope(&self) -> u8 {
        self.envelope.read()
    }

    pub(crate) fn write_poly(&mut self, value: u8) {
        self.clock_shift = value >> 4;
        self.width_7bit = value & 0x08 != 0;
        self.divisor_code = value & 0x07;
    }

    pub(crate) fn read_poly(&self) -> u8 {
        (self.clock_shift << 4) | ((self.width_7bit as u8) << 3) | self.divisor_code
    }

    pub(crate) fn write_control(&mut self, value: u8) {
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
        self.lfsr = 0x7FFF;
    }

    fn period(&self) -> u32 {
        (DIVISORS[self.divisor_code as usize] as u32) << self.clock_shift
    }

    pub(crate) fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            self.freq_timer = self.period();
            self.step_lfsr();
        }
    }

    /// XOR the low two bits back into bit 14 (and bit 6 in 7-bit mode).
    fn step_lfsr(&mut self) {
        let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
        self.lfsr >>= 1;
        self.lfsr |= bit << 14;
        if self.width_7bit {
            self.lfsr = (self.lfsr & !(1 << 6)) | (bit << 6);
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

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn digital(&self) -> u8 {
        if self.enabled {
            // The channel is high when the LFSR's low bit is 0.
            ((!self.lfsr & 1) as u8) * self.envelope.volume()
        } else {
            0
        }
    }

    pub(crate) fn dac_output(&self) -> f32 {
        dac_output(self.digital(), self.envelope.dac_enabled())
    }

    pub(crate) fn reset(&mut self) {
        *self = NoiseChannel::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The low output bit (`!lfsr & 1`) over the first `n` LFSR steps.
    fn output_bits(width_7bit: bool, n: usize) -> Vec<u8> {
        let mut ch = NoiseChannel::new();
        ch.write_poly(if width_7bit { 0x08 } else { 0x00 });
        ch.write_envelope(0xF0); // DAC on, volume 15
        ch.write_control(0x80); // trigger -> lfsr = 0x7FFF
        let mut out = Vec::new();
        for _ in 0..n {
            out.push((!ch.lfsr & 1) as u8);
            ch.step_lfsr();
        }
        out
    }

    #[test]
    fn period_is_divisor_shifted_by_clock_shift() {
        for (code, shift, expected) in [(0u8, 0u8, 8u32), (0, 1, 16), (3, 2, 48 << 2), (7, 4, 112 << 4)]
        {
            let mut ch = NoiseChannel::new();
            ch.write_poly((shift << 4) | code);
            assert_eq!(ch.period(), expected, "code {code} shift {shift}");
        }
    }

    #[test]
    fn fifteen_bit_lfsr_sequence() {
        // Seed 0x7FFF (bit 0 = 1) makes the inverted output start low; the
        // feedback bit shifted into bit 14 takes 15 steps to reach bit 0, so the
        // output is 0 for the first fifteen steps and 1 on the sixteenth.
        let bits = output_bits(false, 16);
        assert_eq!(bits, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn seven_bit_lfsr_repeats_with_period_127() {
        // A 7-bit LFSR cycles every 127 steps; a 15-bit one does not.
        let bits = output_bits(true, 200);
        assert_eq!(bits[0..73], bits[127..200], "7-bit LFSR is periodic at 127");
        let wide = output_bits(false, 200);
        assert_ne!(wide[0..73], wide[127..200], "15-bit LFSR is not periodic at 127");
    }

    #[test]
    fn dac_off_disables_channel_and_blocks_trigger() {
        let mut ch = NoiseChannel::new();
        ch.write_envelope(0x00); // DAC off
        ch.write_control(0x80);
        assert!(!ch.is_enabled());
        assert_eq!(ch.dac_output(), 0.0);
    }

    #[test]
    fn length_expiry_disables_channel() {
        let mut ch = NoiseChannel::new();
        ch.write_envelope(0xF0);
        ch.write_length(63); // counter = 1
        ch.write_control(0xC0); // trigger + length enable
        assert!(ch.is_enabled());
        ch.clock_length();
        assert!(!ch.is_enabled());
    }
}
