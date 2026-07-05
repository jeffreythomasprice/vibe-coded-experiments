//! Channel 3: the wave channel, which plays 32 4-bit samples from Wave RAM
//! (`0xFF30-0xFF3F`) at a programmable frequency and output level.
//!
//! Unlike the square and noise channels its DAC is a single bit (`NR30` bit 7)
//! rather than an envelope, and it has no volume envelope — the output level is
//! a coarse shift (`NR32`).

use super::components::{dac_output, LengthCounter};

/// Right-shift applied to each 4-bit sample per `NR32`'s output-level code:
/// mute, 100%, 50%, 25%.
const VOLUME_SHIFT: [u8; 4] = [4, 0, 1, 2];

#[derive(Debug, Clone)]
pub(crate) struct WaveChannel {
    enabled: bool,
    dac_enabled: bool,
    length: LengthCounter,
    volume_code: u8,
    frequency: u16,
    freq_timer: u16,
    position: u8,
    sample_buffer: u8,
    wave_ram: [u8; 16],
}

impl WaveChannel {
    pub(crate) fn new() -> WaveChannel {
        WaveChannel {
            enabled: false,
            dac_enabled: false,
            length: LengthCounter::new(256),
            volume_code: 0,
            frequency: 0,
            freq_timer: 0,
            position: 0,
            sample_buffer: 0,
            wave_ram: [0; 16],
        }
    }

    pub(crate) fn write_dac(&mut self, value: u8) {
        self.dac_enabled = value & 0x80 != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    pub(crate) fn read_dac(&self) -> u8 {
        ((self.dac_enabled as u8) << 7) | 0x7F
    }

    pub(crate) fn write_length(&mut self, value: u8) {
        self.length.set_length(value);
    }

    pub(crate) fn write_level(&mut self, value: u8) {
        self.volume_code = (value >> 5) & 0x03;
    }

    pub(crate) fn read_level(&self) -> u8 {
        (self.volume_code << 5) | 0x9F
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

    /// Wave RAM (`0xFF30-0xFF3F`) is directly readable and writable, even while
    /// the APU is powered off.
    pub(crate) fn read_wave(&self, addr: u16) -> u8 {
        self.wave_ram[(addr - 0xFF30) as usize]
    }

    pub(crate) fn write_wave(&mut self, addr: u16, value: u8) {
        self.wave_ram[(addr - 0xFF30) as usize] = value;
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled;
        self.freq_timer = self.period();
        self.length.trigger();
        self.position = 0;
        self.sample_buffer = self.wave_ram[0] >> 4;
    }

    /// T-cycles per sample: `(2048 - frequency) * 2`.
    fn period(&self) -> u16 {
        (2048 - self.frequency) * 2
    }

    pub(crate) fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            self.freq_timer = self.period();
            self.position = (self.position + 1) & 31;
            let byte = self.wave_ram[(self.position / 2) as usize];
            self.sample_buffer = if self.position & 1 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
        }
    }

    pub(crate) fn clock_length(&mut self) {
        if self.length.tick() {
            self.enabled = false;
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn digital(&self) -> u8 {
        if self.enabled {
            self.sample_buffer >> VOLUME_SHIFT[self.volume_code as usize]
        } else {
            0
        }
    }

    pub(crate) fn dac_output(&self) -> f32 {
        dac_output(self.digital(), self.dac_enabled)
    }

    pub(crate) fn reset(&mut self) {
        // Powering off clears the channel but not Wave RAM.
        let wave_ram = self.wave_ram;
        *self = WaveChannel::new();
        self.wave_ram = wave_ram;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_ramp() -> WaveChannel {
        let mut ch = WaveChannel::new();
        // Wave RAM byte i holds samples 2i and 2i+1 in its high/low nibbles, so
        // sample n reads back as `n & 0x0F` (a 4-bit nibble can't hold 16-31).
        for i in 0..16u16 {
            let hi = ((i * 2) & 0x0F) as u8;
            let lo = ((i * 2 + 1) & 0x0F) as u8;
            ch.write_wave(0xFF30 + i, (hi << 4) | lo);
        }
        ch
    }

    #[test]
    fn position_advances_through_all_32_samples_high_nibble_first() {
        let mut ch = with_ramp();
        ch.write_dac(0x80);
        ch.write_level(0x20); // 100% output
        ch.write_freq_lo(0xFF);
        ch.write_control(0x80 | 0x07); // trigger, frequency 0x7FF -> period 2
        // After trigger the sample buffer holds sample 0 (high nibble of byte 0).
        assert_eq!(ch.digital(), 0);
        for pos in 1..32u8 {
            ch.tick();
            ch.tick(); // period is 2 T-cycles
            assert_eq!(ch.digital(), pos & 0x0F, "sample at position {pos}");
        }
        // Wraps back to sample 0.
        ch.tick();
        ch.tick();
        assert_eq!(ch.digital(), 0);
    }

    #[test]
    fn output_level_shifts_the_sample() {
        // sample buffer = 12 (0b1100); mute/100/50/25% -> 0/12/6/3.
        for (code, expected) in [(0u8, 0u8), (1, 12), (2, 6), (3, 3)] {
            let mut ch = WaveChannel::new();
            ch.write_wave(0xFF30, 0xC0); // sample 0 = high nibble = 12
            ch.write_dac(0x80);
            ch.write_level(code << 5);
            ch.write_control(0x80); // trigger loads sample 0 (=12)
            assert_eq!(ch.digital(), expected, "level code {code}");
        }
    }

    #[test]
    fn dac_off_disables_channel_and_blocks_trigger() {
        let mut ch = with_ramp();
        ch.write_dac(0x00); // DAC off
        ch.write_control(0x80); // attempt trigger
        assert!(!ch.is_enabled());
        assert_eq!(ch.dac_output(), 0.0);
    }

    #[test]
    fn length_counter_is_256_steps() {
        let mut ch = WaveChannel::new();
        ch.write_dac(0x80);
        ch.write_length(255); // counter = 1
        ch.write_control(0xC0); // trigger + length enable
        assert!(ch.is_enabled());
        ch.clock_length();
        assert!(!ch.is_enabled());
    }

    #[test]
    fn wave_ram_survives_reset() {
        let mut ch = with_ramp();
        ch.write_dac(0x80);
        ch.write_control(0x80);
        ch.reset();
        assert!(!ch.is_enabled());
        assert_eq!(ch.read_wave(0xFF31), (2 << 4) | 3); // ramp value intact
    }

    #[test]
    fn wave_ram_writable_while_powered_off() {
        // The channel never being triggered stands in for "APU powered off": Wave
        // RAM read/write is unconditional here.
        let mut ch = WaveChannel::new();
        for addr in [0xFF30u16, 0xFF3F] {
            ch.write_wave(addr, 0xA5);
            assert_eq!(ch.read_wave(addr), 0xA5);
        }
    }
}
