//! The Game Boy (DMG) audio processing unit (APU), registers `0xFF10-0xFF3F`.
//!
//! Like the timer/PPU/serial, [`Apu`] is a self-contained subsystem owned by
//! [`SystemBus`](crate::memory::SystemBus) and advanced by its `tick`. Unlike
//! them it raises **no interrupt** — its output is a stream of stereo audio
//! samples, drained via [`Apu::take_samples`] (the analog of the PPU's
//! `take_frame_ready`/`framebuffer` seam).
//!
//! It owns the four sound channels (two square, one wave, one noise; see the
//! `square`/`wave`/`noise` modules), the [`FrameSequencer`] that clocks their
//! length/envelope/sweep units, and the master control registers `NR50`/`NR51`/
//! `NR52`. Each channel is stepped one T-cycle at a time (like the timer), and a
//! fractional accumulator emits one interleaved stereo sample every
//! `CLOCK_HZ / APU_SAMPLE_RATE` T-cycles.
//!
//! Power-on state is all-zero (we skip the boot ROM); post-boot register *read*
//! values come from per-register forced-1 masks rather than seeded storage, the
//! same convention as the timer/serial. Resampling the fixed
//! [`APU_SAMPLE_RATE`] stream to a host device rate happens outside this crate
//! (in the `audio` crate); the APU only produces the intermediate-rate samples.

mod components;
mod frame_sequencer;
mod noise;
mod square;
mod wave;

use crate::CLOCK_HZ;
use frame_sequencer::FrameSequencer;
use noise::NoiseChannel;
use square::SquareChannel;
use wave::WaveChannel;

/// The fixed rate (Hz) the APU emits stereo samples at. The `audio` crate
/// resamples this to the actual output device rate.
pub const APU_SAMPLE_RATE: u32 = 48_000;

/// The DMG audio processing unit. See the module docs for the model.
#[derive(Debug, Clone)]
pub struct Apu {
    ch1: SquareChannel,
    ch2: SquareChannel,
    ch3: WaveChannel,
    ch4: NoiseChannel,
    sequencer: FrameSequencer,
    /// `NR50` (`0xFF24`): master L/R volume and Vin routing (stored verbatim).
    nr50: u8,
    /// `NR51` (`0xFF25`): per-channel L/R routing (stored verbatim).
    nr51: u8,
    /// `NR52` (`0xFF26`) bit 7: master power. When off, the channels are held
    /// disabled and most registers ignore writes.
    powered: bool,
    /// Fractional accumulator for the sample clock.
    sample_accum: u32,
    /// Interleaved stereo samples (`L, R, L, R, …`) produced since the last drain.
    samples: Vec<f32>,
}

impl Default for Apu {
    fn default() -> Apu {
        Apu::new()
    }
}

impl Apu {
    pub fn new() -> Apu {
        Apu {
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            ch3: WaveChannel::new(),
            ch4: NoiseChannel::new(),
            sequencer: FrameSequencer::new(),
            nr50: 0,
            nr51: 0,
            powered: false,
            sample_accum: 0,
            samples: Vec::new(),
        }
    }

    /// Read one of the APU registers (`0xFF10-0xFF3F`). Unused bits read back as
    /// `1` per the per-register masks; write-only registers read `0xFF`.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => self.ch1.read_sweep(),
            0xFF11 => self.ch1.read_length_duty(),
            0xFF12 => self.ch1.read_envelope(),
            0xFF13 => 0xFF,
            0xFF14 => self.ch1.read_control(),
            0xFF15 => 0xFF,
            0xFF16 => self.ch2.read_length_duty(),
            0xFF17 => self.ch2.read_envelope(),
            0xFF18 => 0xFF,
            0xFF19 => self.ch2.read_control(),
            0xFF1A => self.ch3.read_dac(),
            0xFF1B => 0xFF,
            0xFF1C => self.ch3.read_level(),
            0xFF1D => 0xFF,
            0xFF1E => self.ch3.read_control(),
            0xFF1F => 0xFF,
            0xFF20 => 0xFF,
            0xFF21 => self.ch4.read_envelope(),
            0xFF22 => self.ch4.read_poly(),
            0xFF23 => self.ch4.read_control(),
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => self.read_nr52(),
            0xFF27..=0xFF2F => 0xFF,
            0xFF30..=0xFF3F => self.ch3.read_wave(addr),
            _ => unreachable!("apu read of non-apu address {addr:#06x}"),
        }
    }

    /// Write one of the APU registers (`0xFF10-0xFF3F`).
    ///
    /// `NR52` (power) and Wave RAM are always writable; while the APU is powered
    /// off every other register write is ignored.
    pub fn write(&mut self, addr: u16, value: u8) {
        if addr == 0xFF26 {
            self.write_nr52(value);
            return;
        }
        if (0xFF30..=0xFF3F).contains(&addr) {
            self.ch3.write_wave(addr, value);
            return;
        }
        if !self.powered {
            return;
        }
        match addr {
            0xFF10 => self.ch1.write_sweep(value),
            0xFF11 => self.ch1.write_length_duty(value),
            0xFF12 => self.ch1.write_envelope(value),
            0xFF13 => self.ch1.write_freq_lo(value),
            0xFF14 => self.ch1.write_control(value),
            0xFF15 => {}
            0xFF16 => self.ch2.write_length_duty(value),
            0xFF17 => self.ch2.write_envelope(value),
            0xFF18 => self.ch2.write_freq_lo(value),
            0xFF19 => self.ch2.write_control(value),
            0xFF1A => self.ch3.write_dac(value),
            0xFF1B => self.ch3.write_length(value),
            0xFF1C => self.ch3.write_level(value),
            0xFF1D => self.ch3.write_freq_lo(value),
            0xFF1E => self.ch3.write_control(value),
            0xFF1F => {}
            0xFF20 => self.ch4.write_length(value),
            0xFF21 => self.ch4.write_envelope(value),
            0xFF22 => self.ch4.write_poly(value),
            0xFF23 => self.ch4.write_control(value),
            0xFF24 => self.nr50 = value,
            0xFF25 => self.nr51 = value,
            0xFF27..=0xFF2F => {}
            _ => unreachable!("apu write of non-apu address {addr:#06x}"),
        }
    }

    /// `NR52`: bit 7 = power, bits 4-6 read as `1`, bits 0-3 are the live
    /// per-channel enabled flags (read-only).
    fn read_nr52(&self) -> u8 {
        let mut value = 0x70;
        if self.powered {
            value |= 0x80;
        }
        value |= self.ch1.is_enabled() as u8;
        value |= (self.ch2.is_enabled() as u8) << 1;
        value |= (self.ch3.is_enabled() as u8) << 2;
        value |= (self.ch4.is_enabled() as u8) << 3;
        value
    }

    fn write_nr52(&mut self, value: u8) {
        let power = value & 0x80 != 0;
        if power == self.powered {
            return;
        }
        self.powered = power;
        if power {
            tracing::debug!("apu powered on");
        } else {
            self.power_off();
            tracing::debug!("apu powered off");
        }
    }

    /// Powering off clears the channel and control registers (Wave RAM survives)
    /// and resets the frame sequencer.
    fn power_off(&mut self) {
        self.ch1.reset();
        self.ch2.reset();
        self.ch3.reset();
        self.ch4.reset();
        self.nr50 = 0;
        self.nr51 = 0;
        self.sequencer = FrameSequencer::new();
    }

    /// Advance the APU by `t_states` T-cycles, generating audio samples. Raises
    /// no interrupt.
    pub fn tick(&mut self, t_states: u32) {
        for _ in 0..t_states {
            self.tick_one();
        }
    }

    fn tick_one(&mut self) {
        if self.powered {
            let events = self.sequencer.tick_one();
            if events.length {
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
                self.ch4.clock_length();
            }
            if events.sweep {
                self.ch1.clock_sweep();
            }
            if events.envelope {
                self.ch1.clock_envelope();
                self.ch2.clock_envelope();
                self.ch4.clock_envelope();
            }
            self.ch1.tick();
            self.ch2.tick();
            self.ch3.tick();
            self.ch4.tick();
        }

        // The sample clock runs regardless of power so the output keeps a steady
        // rate; a powered-off APU simply emits silence.
        self.sample_accum += APU_SAMPLE_RATE;
        if self.sample_accum >= CLOCK_HZ {
            self.sample_accum -= CLOCK_HZ;
            let (left, right) = if self.powered { self.mix() } else { (0.0, 0.0) };
            self.samples.push(left);
            self.samples.push(right);
        }
    }

    /// Mix the four channels into a stereo pair in `[-1.0, 1.0]` per side,
    /// applying `NR51` routing and `NR50` master volume.
    fn mix(&self) -> (f32, f32) {
        let outputs = [
            self.ch1.dac_output(),
            self.ch2.dac_output(),
            self.ch3.dac_output(),
            self.ch4.dac_output(),
        ];
        let mut left = 0.0;
        let mut right = 0.0;
        for (i, &sample) in outputs.iter().enumerate() {
            if self.nr51 & (1 << (4 + i)) != 0 {
                left += sample;
            }
            if self.nr51 & (1 << i) != 0 {
                right += sample;
            }
        }
        // 3-bit master volume per side, 0-7 mapped to (vol+1)/8; dividing the
        // four-channel sum by 4 keeps the result within [-1.0, 1.0].
        let left_vol = ((self.nr50 >> 4) & 0x07) as f32 + 1.0;
        let right_vol = (self.nr50 & 0x07) as f32 + 1.0;
        (left / 4.0 * (left_vol / 8.0), right / 4.0 * (right_vol / 8.0))
    }

    /// Drain the interleaved stereo samples (`L, R, …`) produced since the last
    /// call, leaving the internal buffer empty.
    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A powered-on APU (as a game would leave it after writing `NR52` bit 7).
    fn powered() -> Apu {
        let mut apu = Apu::new();
        apu.write(0xFF26, 0x80);
        apu
    }

    #[test]
    fn power_on_read_values() {
        // Post-boot (powered off) NR52 reads 0x70 (forced bits 4-6, power off).
        let apu = Apu::new();
        assert_eq!(apu.read(0xFF26), 0x70);
    }

    #[test]
    fn register_read_masks() {
        // (addr, value written, expected read) with the APU powered so writes land.
        let cases = [
            (0xFF10u16, 0x00u8, 0x80u8), // NR10: bit 7 forced
            (0xFF11, 0x00, 0x3F),        // NR11: only duty readable
            (0xFF12, 0xA5, 0xA5),        // NR12: fully readable
            (0xFF13, 0xFF, 0xFF),        // NR13: write-only
            (0xFF14, 0x00, 0xBF),        // NR14: only length-enable readable (here 0)
            (0xFF1A, 0x00, 0x7F),        // NR30: only DAC bit readable
            (0xFF1C, 0x00, 0x9F),        // NR32: only level readable
            (0xFF20, 0x3F, 0xFF),        // NR41: write-only
            (0xFF23, 0x00, 0xBF),        // NR44: only length-enable readable (here 0)
            (0xFF25, 0x5A, 0x5A),        // NR51: fully readable
        ];
        let mut apu = powered();
        for (addr, value, expected) in cases {
            apu.write(addr, value);
            assert_eq!(apu.read(addr), expected, "register {addr:#06x}");
        }
    }

    #[test]
    fn unused_region_reads_ff() {
        let apu = powered();
        for addr in [0xFF27u16, 0xFF2F] {
            assert_eq!(apu.read(addr), 0xFF);
        }
    }

    #[test]
    fn nr52_reports_channel_status() {
        let mut apu = powered();
        // Trigger channel 1 (square with envelope DAC on).
        apu.write(0xFF12, 0xF0); // NR12: DAC on
        apu.write(0xFF14, 0x80); // NR14: trigger
        assert_eq!(apu.read(0xFF26) & 0x01, 0x01, "ch1 status set after trigger");

        // NR52 bits 0-3 are read-only: writing them does not fake a status.
        let mut apu = powered();
        apu.write(0xFF26, 0x8F);
        assert_eq!(apu.read(0xFF26) & 0x0F, 0x00);
    }

    #[test]
    fn power_off_clears_registers_but_not_wave_ram() {
        let mut apu = powered();
        apu.write(0xFF24, 0x77); // NR50
        apu.write(0xFF25, 0x55); // NR51
        apu.write(0xFF30, 0xAB); // Wave RAM
        apu.write(0xFF12, 0xF0);
        apu.write(0xFF14, 0x80); // trigger ch1

        apu.write(0xFF26, 0x00); // power off
        assert_eq!(apu.read(0xFF24), 0x00, "NR50 cleared");
        assert_eq!(apu.read(0xFF25), 0x00, "NR51 cleared");
        assert_eq!(apu.read(0xFF26), 0x70, "all channels off, power off");
        assert_eq!(apu.read(0xFF30), 0xAB, "Wave RAM preserved");
    }

    #[test]
    fn writes_ignored_while_powered_off() {
        let mut apu = Apu::new(); // powered off
        apu.write(0xFF12, 0xF0);
        assert_eq!(apu.read(0xFF12), 0x00, "NR12 write ignored while off");
        // Wave RAM is the exception: it is always writable.
        apu.write(0xFF30, 0xCD);
        assert_eq!(apu.read(0xFF30), 0xCD);
    }

    #[test]
    fn routing_and_volume_place_a_channel_on_one_side() {
        let mut apu = powered();
        // Channel 1: a 50% duty square at max volume, so it produces non-silent
        // samples. Route it to the LEFT only, full left master volume.
        apu.write(0xFF11, 0x80); // NR11: duty 50%
        apu.write(0xFF12, 0xF0); // NR12: volume 15, DAC on
        apu.write(0xFF13, 0x00); // NR13: freq lo
        apu.write(0xFF14, 0x87); // NR14: trigger, freq hi = 7
        apu.write(0xFF25, 0x10); // NR51: ch1 -> left only
        apu.write(0xFF24, 0x70); // NR50: left volume 7, right 0

        apu.tick(CLOCK_HZ / 64); // a chunk of samples
        let samples = apu.take_samples();
        assert!(!samples.is_empty());
        let right_nonzero = samples.iter().skip(1).step_by(2).any(|&s| s != 0.0);
        assert!(!right_nonzero, "right side is silent (ch1 routed left only)");
        let left_nonzero = samples.iter().step_by(2).any(|&s| s != 0.0);
        assert!(left_nonzero, "left side carries the channel");
    }

    #[test]
    fn take_samples_drains_the_buffer() {
        let mut apu = powered();
        apu.tick(CLOCK_HZ); // one second of audio
        let first = apu.take_samples();
        assert!(!first.is_empty());
        // A full second yields about APU_SAMPLE_RATE stereo pairs.
        let pairs = first.len() / 2;
        let tolerance = 2;
        assert!(
            (pairs as i64 - APU_SAMPLE_RATE as i64).abs() <= tolerance,
            "got {pairs} pairs, expected ~{APU_SAMPLE_RATE}"
        );
        assert!(apu.take_samples().is_empty(), "second drain is empty");
    }
}
