//! The APU frame sequencer: a 512 Hz clock derived from the main T-cycle clock
//! that paces the length counters (256 Hz), volume envelopes (64 Hz), and
//! channel-1 frequency sweep (128 Hz).
//!
//! It is stepped one T-cycle at a time by [`super::Apu`] (like the timer and
//! PPU), so each of the eight sub-steps lands on the exact cycle even when an
//! instruction advances the clock by many T-cycles.

/// T-cycles between frame-sequencer steps: `4194304 / 512`.
const PERIOD: u32 = 8192;

/// Which clocks fire on a given frame-sequencer step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FrameEvents {
    pub(crate) length: bool,
    pub(crate) envelope: bool,
    pub(crate) sweep: bool,
}

/// The 8-step frame sequencer. Length fires on steps 0/2/4/6 (256 Hz), sweep on
/// 2/6 (128 Hz), and envelope on 7 (64 Hz).
#[derive(Debug, Clone)]
pub(crate) struct FrameSequencer {
    counter: u32,
    step: u8,
}

impl FrameSequencer {
    pub(crate) fn new() -> FrameSequencer {
        FrameSequencer { counter: 0, step: 0 }
    }

    /// Advance a single T-cycle, returning the clocks that fire if this cycle
    /// crossed a step boundary (empty otherwise).
    pub(crate) fn tick_one(&mut self) -> FrameEvents {
        self.counter += 1;
        if self.counter < PERIOD {
            return FrameEvents::default();
        }
        self.counter = 0;
        let events = events_for_step(self.step);
        self.step = (self.step + 1) % 8;
        events
    }
}

fn events_for_step(step: u8) -> FrameEvents {
    FrameEvents {
        length: matches!(step, 0 | 2 | 4 | 6),
        sweep: matches!(step, 2 | 6),
        envelope: step == 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Advance `cycles` T-cycles, returning the events OR-ed across the span.
    fn run(seq: &mut FrameSequencer, cycles: u32) -> FrameEvents {
        let mut acc = FrameEvents::default();
        for _ in 0..cycles {
            let e = seq.tick_one();
            acc.length |= e.length;
            acc.envelope |= e.envelope;
            acc.sweep |= e.sweep;
        }
        acc
    }

    #[test]
    fn nothing_fires_before_the_period_boundary() {
        let mut seq = FrameSequencer::new();
        assert_eq!(run(&mut seq, PERIOD - 1), FrameEvents::default());
        // The 8192nd T-cycle crosses into step 0 and fires length.
        assert_eq!(
            seq.tick_one(),
            FrameEvents {
                length: true,
                envelope: false,
                sweep: false
            }
        );
    }

    #[test]
    fn eight_step_cadence() {
        // The clocks fired at each of the eight steps, in order.
        let expected = [
            (true, false, false),  // 0: length
            (false, false, false), // 1: nothing
            (true, false, true),   // 2: length + sweep
            (false, false, false), // 3: nothing
            (true, false, false),  // 4: length
            (false, false, false), // 5: nothing
            (true, false, true),   // 6: length + sweep
            (false, true, false),  // 7: envelope
        ];
        let mut seq = FrameSequencer::new();
        for (i, (length, envelope, sweep)) in expected.into_iter().enumerate() {
            let events = run(&mut seq, PERIOD);
            assert_eq!(events.length, length, "step {i} length");
            assert_eq!(events.envelope, envelope, "step {i} envelope");
            assert_eq!(events.sweep, sweep, "step {i} sweep");
        }
    }

    #[test]
    fn length_is_256hz_sweep_128hz_envelope_64hz() {
        // Over one full 8-step cycle: length 4×, sweep 2×, envelope 1×.
        let mut seq = FrameSequencer::new();
        let (mut length, mut sweep, mut envelope) = (0, 0, 0);
        for _ in 0..8 {
            let e = run(&mut seq, PERIOD);
            length += e.length as u32;
            sweep += e.sweep as u32;
            envelope += e.envelope as u32;
        }
        assert_eq!((length, sweep, envelope), (4, 2, 1));
    }
}
