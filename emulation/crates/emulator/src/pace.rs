//! Wall-clock pacing for the emulation run loop.
//!
//! The `gameboy` core advances *emulated* time one frame at a time
//! ([`gameboy::Emulator::run_frame`]); [`Pacer`] decides *when* (in real time) to
//! run those frames so the machine runs at the configured [`SpeedMode`]:
//!
//! - [`SpeedMode::Native`] — one frame every ~16.74 ms, matching the DMG's
//!   ~59.7275 Hz refresh.
//! - [`SpeedMode::Relative`] — that period divided by the factor (2.0× → half the
//!   period). If the host can't keep up it simply runs as fast as it can.
//! - [`SpeedMode::Unbounded`] — no pacing; run a frame every loop iteration.
//!
//! The pacer also owns the lag reporting: it logs the mode once at construction,
//! warns when it starts consistently falling behind its target (with hysteresis
//! so a single stutter doesn't trip it), repeats that warning at most once a
//! minute while behind, and logs a recovery when it catches back up.

use std::time::{Duration, Instant};

use common::SpeedMode;
use winit::event_loop::ControlFlow;

/// Most frames to run in a single catch-up burst before giving up and resyncing.
/// Absorbs a stalled scheduler slice without letting a slow host accumulate an
/// unbounded backlog of owed frames.
const MAX_CATCHUP: u32 = 8;

/// How long the machine must be continuously behind (or continuously caught up)
/// before the lag state flips — hysteresis against single-frame jitter.
const LAG_HYSTERESIS: Duration = Duration::from_millis(750);

/// While lagging, re-log at most this often.
const LAG_REPEAT: Duration = Duration::from_secs(60);

/// The base frame period at native speed, derived from the master clock and the
/// T-cycles in one PPU frame (≈ 16_742_706 ns).
fn native_period() -> Duration {
    let nanos = 1_000_000_000u64 * gameboy::T_CYCLES_PER_FRAME as u64 / gameboy::CLOCK_HZ as u64;
    Duration::from_nanos(nanos)
}

/// The target wall-clock period between frames for a mode, or `None` for
/// [`SpeedMode::Unbounded`] (no pacing).
fn frame_period(mode: SpeedMode) -> Option<Duration> {
    match mode {
        SpeedMode::Native => Some(native_period()),
        SpeedMode::Relative(factor) if factor > 0.0 => Some(native_period().div_f32(factor)),
        SpeedMode::Relative(factor) => {
            tracing::warn!(factor, "non-positive speed factor; treating as native");
            Some(native_period())
        }
        SpeedMode::Unbounded => None,
    }
}

/// Paces emulated frames against the wall clock per a [`SpeedMode`].
pub struct Pacer {
    mode: SpeedMode,
    /// The target period per frame, or `None` when unbounded.
    period: Option<Duration>,
    /// The wall-clock time the next frame is due. Armed lazily on the first
    /// paced wake so the clock doesn't start before the window exists.
    next_deadline: Option<Instant>,
    /// When we first fell behind (reset whenever we're on time).
    behind_since: Option<Instant>,
    /// When we first caught up again while flagged as lagging.
    ontime_since: Option<Instant>,
    /// Whether we're currently reporting as lagging.
    lagging: bool,
    /// When we last emitted a lag warning (to rate-limit the repeats).
    last_lag_log: Option<Instant>,
}

impl Pacer {
    /// Build a pacer for `mode`, logging the chosen mode once.
    pub fn new(mode: SpeedMode) -> Pacer {
        match mode {
            SpeedMode::Native => tracing::info!("emulation speed: native (~59.7 Hz)"),
            SpeedMode::Relative(factor) => {
                tracing::info!(factor, "emulation speed: {factor}x native")
            }
            SpeedMode::Unbounded => tracing::info!("emulation speed: unbounded (as fast as possible)"),
        }
        Pacer {
            mode,
            period: frame_period(mode),
            next_deadline: None,
            behind_since: None,
            ontime_since: None,
            lagging: false,
            last_lag_log: None,
        }
    }

    /// Whether audio samples should be sent to the device in this mode. Only near
    /// real time is playback meaningful; faster modes would just produce
    /// time-compressed noise, so we mute them (the samples are still drained from
    /// the core each frame, they're just not submitted).
    pub fn submit_audio(&self) -> bool {
        match self.mode {
            SpeedMode::Native => true,
            SpeedMode::Relative(factor) => (0.9..=1.1).contains(&factor),
            SpeedMode::Unbounded => false,
        }
    }

    /// The control flow to arm the event loop with until the next wake: spin
    /// (`Poll`) when unbounded, otherwise sleep until the next frame is due.
    pub fn control_flow(&self) -> ControlFlow {
        match self.next_deadline {
            Some(deadline) => ControlFlow::WaitUntil(deadline),
            None => ControlFlow::Poll,
        }
    }

    /// Run as many frames as the schedule currently owes, invoking `run_frame`
    /// once per emulated frame. Call this every loop iteration; it does nothing
    /// when a paced frame isn't due yet.
    pub fn tick(&mut self, mut run_frame: impl FnMut()) {
        let Some(period) = self.period else {
            // Unbounded: one frame per iteration, no deadline bookkeeping.
            run_frame();
            return;
        };

        let now = Instant::now();
        let deadline = *self.next_deadline.get_or_insert(now + period);
        if now < deadline {
            return; // not time for the next frame yet
        }

        // Run owed frames, capped so a slow host can't spiral. Re-sample the
        // clock each iteration so the cap reflects real elapsed time.
        let mut frames = 0;
        while frames < MAX_CATCHUP && Instant::now() >= self.deadline() {
            run_frame();
            *self.next_deadline.as_mut().unwrap() += period;
            frames += 1;
        }

        // Still past the deadline after a full catch-up burst → we genuinely
        // can't keep up. Drop the accumulated debt (graceful slowdown, not a
        // spiral) and flag it.
        let after = Instant::now();
        let behind = after >= self.deadline();
        if behind {
            self.next_deadline = Some(after + period);
        }
        self.update_lag(behind, after);
    }

    fn deadline(&self) -> Instant {
        self.next_deadline.expect("deadline armed before catch-up loop")
    }

    /// Fold this wake's keeping-up verdict into the lag state, logging on the
    /// not-lagging→lagging→recovery transitions (with hysteresis and rate limit).
    fn update_lag(&mut self, behind: bool, now: Instant) {
        if behind {
            self.ontime_since = None;
            let since = *self.behind_since.get_or_insert(now);
            if !self.lagging {
                if now.duration_since(since) >= LAG_HYSTERESIS {
                    self.lagging = true;
                    self.last_lag_log = Some(now);
                    tracing::warn!(
                        mode = ?self.mode,
                        "emulation is falling behind its target speed; running as fast as it can"
                    );
                }
            } else if self
                .last_lag_log
                .is_none_or(|t| now.duration_since(t) >= LAG_REPEAT)
            {
                self.last_lag_log = Some(now);
                tracing::warn!(mode = ?self.mode, "emulation is still behind its target speed");
            }
        } else {
            self.behind_since = None;
            if self.lagging {
                let since = *self.ontime_since.get_or_insert(now);
                if now.duration_since(since) >= LAG_HYSTERESIS {
                    self.lagging = false;
                    self.ontime_since = None;
                    self.last_lag_log = None;
                    tracing::info!("emulation has caught back up to its target speed");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_period_is_about_16_7_ms() {
        let p = native_period();
        assert_eq!(p.as_nanos(), 16_742_706);
    }

    #[test]
    fn relative_halves_or_doubles_the_period() {
        let base = native_period();
        assert_eq!(frame_period(SpeedMode::Relative(2.0)), Some(base.div_f32(2.0)));
        assert_eq!(frame_period(SpeedMode::Unbounded), None);
    }

    #[test]
    fn non_positive_factor_falls_back_to_native() {
        assert_eq!(frame_period(SpeedMode::Relative(0.0)), Some(native_period()));
        assert_eq!(frame_period(SpeedMode::Relative(-1.0)), Some(native_period()));
    }

    #[test]
    fn audio_is_muted_outside_real_time() {
        assert!(Pacer::new(SpeedMode::Native).submit_audio());
        assert!(Pacer::new(SpeedMode::Relative(1.0)).submit_audio());
        assert!(!Pacer::new(SpeedMode::Relative(2.0)).submit_audio());
        assert!(!Pacer::new(SpeedMode::Unbounded).submit_audio());
    }

    #[test]
    fn lag_flips_on_only_after_sustained_slowness() {
        let mut pacer = Pacer::new(SpeedMode::Native);
        let t0 = Instant::now();
        // A brief hitch under the hysteresis window must not flag lag.
        pacer.update_lag(true, t0);
        pacer.update_lag(true, t0 + Duration::from_millis(500));
        assert!(!pacer.lagging);
        // Sustained past the window: now it flags.
        pacer.update_lag(true, t0 + Duration::from_millis(800));
        assert!(pacer.lagging);
    }

    #[test]
    fn slow_host_is_detected_as_lagging() {
        // A host that takes far longer than the target period per frame must flip
        // to lagging within the hysteresis window.
        let mut pacer = Pacer::new(SpeedMode::Relative(50.0));
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(1000) && !pacer.lagging {
            pacer.tick(|| std::thread::sleep(Duration::from_millis(3)));
        }
        assert!(pacer.lagging, "a host slower than the target should be flagged");
    }

    #[test]
    fn a_single_ontime_wake_does_not_immediately_clear_lag() {
        let mut pacer = Pacer::new(SpeedMode::Native);
        let t0 = Instant::now();
        pacer.update_lag(true, t0);
        pacer.update_lag(true, t0 + Duration::from_secs(1));
        assert!(pacer.lagging);
        // Recovery also needs to be sustained past the hysteresis window.
        pacer.update_lag(false, t0 + Duration::from_secs(2));
        assert!(pacer.lagging, "one on-time wake shouldn't clear the lag flag");
        pacer.update_lag(false, t0 + Duration::from_secs(3));
        assert!(!pacer.lagging, "sustained on-time wakes clear it");
    }
}
