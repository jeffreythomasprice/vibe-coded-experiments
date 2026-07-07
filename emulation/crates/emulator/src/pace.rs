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
//! - [`SpeedMode::Unbounded`] — no pacing; run a wall-clock-boxed *burst* of
//!   frames each loop iteration (the event loop yields only ~display-rate
//!   iterations, so one frame per iteration would cap us at the refresh rate).
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

/// Wall-clock budget for one unbounded ("as fast as possible") burst before
/// returning to the event loop to service input and presentation. The loop
/// yields only ~display-rate iterations (the compositor paces `about_to_wait`),
/// so emulating a single frame per iteration would pin unbounded mode to the
/// refresh rate — the very reason it measured *slower* than a fixed multiplier.
/// Running a boxed burst instead lets each iteration do as much emulation as the
/// host can while keeping input latency to roughly one frame. Sized to about one
/// native frame so each compositor wakeup is nearly fully utilized.
const UNBOUNDED_BURST_BUDGET: Duration = Duration::from_micros(16_000);

/// Hard cap on frames per unbounded burst — a safety valve so a host that can
/// emulate a frame in near-zero wall-clock time can't spin the budget check
/// indefinitely (nor overflow the sample counter). Never binds at real speeds.
const UNBOUNDED_BURST_MAX: u32 = 4096;

/// How long the machine must be continuously behind (or continuously caught up)
/// before the lag state flips — hysteresis against single-frame jitter.
const LAG_HYSTERESIS: Duration = Duration::from_millis(750);

/// While lagging, re-log at most this often.
const LAG_REPEAT: Duration = Duration::from_secs(60);

/// Wall-clock span each actual-speed sample is averaged over. Long enough to be
/// steady, short enough to react to the host bogging down.
const MEASURE_WINDOW: Duration = Duration::from_millis(500);

/// If no frame ran for this long (paused, or the menu is open), discard the
/// in-progress sample so the idle gap isn't averaged into the next reading.
const MEASURE_GAP_RESET: Duration = Duration::from_millis(250);

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

/// Log the chosen speed mode once (shared by construction and runtime changes).
fn log_mode(mode: SpeedMode) {
    match mode {
        SpeedMode::Native => tracing::info!("emulation speed: native (~59.7 Hz)"),
        SpeedMode::Relative(factor) => tracing::info!(factor, "emulation speed: {factor}x native"),
        SpeedMode::Unbounded => tracing::info!("emulation speed: unbounded (as fast as possible)"),
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
    /// Wall-clock start of the current actual-speed sampling window.
    measure_start: Option<Instant>,
    /// Frames run since the sampling window started.
    measure_frames: u32,
    /// When `tick` last ran, to detect idle gaps (pause/menu) and discard a window.
    last_tick: Option<Instant>,
    /// The most recent measured actual speed (× real time), for the HUD.
    measured_speed: Option<f32>,
}

impl Pacer {
    /// Build a pacer for `mode`, logging the chosen mode once.
    pub fn new(mode: SpeedMode) -> Pacer {
        log_mode(mode);
        Pacer {
            mode,
            period: frame_period(mode),
            next_deadline: None,
            behind_since: None,
            ontime_since: None,
            lagging: false,
            last_lag_log: None,
            measure_start: None,
            measure_frames: 0,
            last_tick: None,
            measured_speed: None,
        }
    }

    /// The speed mode the pacer is currently running at (for the speed HUD).
    pub fn mode(&self) -> SpeedMode {
        self.mode
    }

    /// The most recently measured actual emulation speed as a multiple of real
    /// time (e.g. `2.77` when the machine is running at 2.77× native), or `None`
    /// until the first sampling window completes. Diverging below the target
    /// speed means the host can't keep up.
    pub fn actual_speed(&self) -> Option<f32> {
        self.measured_speed
    }

    /// Switch to a new speed mode at runtime: recompute the target period, re-arm
    /// the schedule fresh, and reset the lag tracking so a mode change doesn't
    /// carry over stale behind/caught-up state.
    pub fn set_mode(&mut self, mode: SpeedMode) {
        log_mode(mode);
        self.mode = mode;
        self.period = frame_period(mode);
        self.next_deadline = None;
        self.behind_since = None;
        self.ontime_since = None;
        self.lagging = false;
        self.last_lag_log = None;
        // The old actual-speed reading is meaningless at the new speed.
        self.measure_start = None;
        self.measure_frames = 0;
        self.last_tick = None;
        self.measured_speed = None;
    }

    /// Drop any armed deadline so the next paced tick re-arms from `now`. Called
    /// when resuming from a pause so the idle interval isn't treated as owed
    /// frames (which would trigger a catch-up burst). Also discards the
    /// in-progress actual-speed window (but keeps the last reading for display).
    pub fn resync(&mut self) {
        self.next_deadline = None;
        self.measure_start = None;
        self.measure_frames = 0;
        self.last_tick = None;
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
            // Unbounded: run a wall-clock-boxed burst rather than a single frame,
            // so throughput isn't capped at the event loop's ~display-rate
            // iteration rate. No deadline bookkeeping — just run until the budget
            // (or the safety cap) is spent, then let the loop present/poll input.
            let start = Instant::now();
            let mut frames = 0;
            loop {
                run_frame();
                frames += 1;
                if frames >= UNBOUNDED_BURST_MAX
                    || Instant::now().duration_since(start) >= UNBOUNDED_BURST_BUDGET
                {
                    break;
                }
            }
            self.account(frames, Instant::now());
            return;
        };

        let now = Instant::now();
        let deadline = *self.next_deadline.get_or_insert(now + period);
        if now < deadline {
            self.account(0, now); // no frame due, but keep the measurement clock moving
            return;
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
        self.account(frames, after);
    }

    /// Fold `frames_ran` (as of wall-clock `now`) into the actual-speed
    /// measurement, publishing a fresh sample each time the window fills. The
    /// speed is `frames × native_period / elapsed` — the emulated frame rate
    /// expressed as a multiple of native (real-time) speed.
    fn account(&mut self, frames_ran: u32, now: Instant) {
        if let Some(last) = self.last_tick {
            if now.duration_since(last) >= MEASURE_GAP_RESET {
                // Idle since the last tick (paused / menu open) — start fresh so
                // the gap isn't counted as slow emulation.
                self.measure_start = None;
                self.measure_frames = 0;
            }
        }
        self.last_tick = Some(now);

        let start = *self.measure_start.get_or_insert(now);
        self.measure_frames += frames_ran;
        let elapsed = now.duration_since(start);
        if elapsed >= MEASURE_WINDOW {
            let speed =
                self.measure_frames as f64 * native_period().as_secs_f64() / elapsed.as_secs_f64();
            self.measured_speed = Some(speed as f32);
            self.measure_start = Some(now);
            self.measure_frames = 0;
        }
    }

    fn deadline(&self) -> Instant {
        self.next_deadline
            .expect("deadline armed before catch-up loop")
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
        assert_eq!(
            frame_period(SpeedMode::Relative(2.0)),
            Some(base.div_f32(2.0))
        );
        assert_eq!(frame_period(SpeedMode::Unbounded), None);
    }

    #[test]
    fn non_positive_factor_falls_back_to_native() {
        assert_eq!(
            frame_period(SpeedMode::Relative(0.0)),
            Some(native_period())
        );
        assert_eq!(
            frame_period(SpeedMode::Relative(-1.0)),
            Some(native_period())
        );
    }

    #[test]
    fn set_mode_updates_period_and_rearms() {
        let mut pacer = Pacer::new(SpeedMode::Native);
        // Arm a deadline, then a mode change must recompute the period and clear it.
        pacer.tick(|| {});
        assert!(pacer.next_deadline.is_some());
        pacer.set_mode(SpeedMode::Relative(2.0));
        assert_eq!(pacer.mode(), SpeedMode::Relative(2.0));
        assert_eq!(pacer.period, frame_period(SpeedMode::Relative(2.0)));
        assert!(pacer.next_deadline.is_none());
        pacer.set_mode(SpeedMode::Unbounded);
        assert_eq!(pacer.period, None);
    }

    #[test]
    fn actual_speed_is_measured_then_cleared_on_mode_change() {
        let mut pacer = Pacer::new(SpeedMode::Unbounded);
        assert!(pacer.actual_speed().is_none());
        // Unbounded runs one frame per tick; loop past the sampling window.
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(800) && pacer.actual_speed().is_none() {
            pacer.tick(|| {});
        }
        let speed = pacer.actual_speed();
        assert!(speed.is_some(), "a sample should land within the window");
        assert!(speed.unwrap() > 0.0);
        // Switching speed invalidates the old reading.
        pacer.set_mode(SpeedMode::Native);
        assert!(pacer.actual_speed().is_none());
    }

    #[test]
    fn unbounded_runs_a_burst_per_tick() {
        // The whole point of the unbounded fix: a single tick must batch many
        // frames, not run exactly one (which would cap throughput at the event
        // loop's iteration rate).
        let mut pacer = Pacer::new(SpeedMode::Unbounded);
        let mut frames = 0u32;
        pacer.tick(|| frames += 1);
        assert!(
            frames > 1,
            "unbounded should batch frames per tick, ran {frames}"
        );
        assert!(frames <= UNBOUNDED_BURST_MAX, "burst must honor the cap");
    }

    #[test]
    fn resync_clears_the_deadline() {
        let mut pacer = Pacer::new(SpeedMode::Native);
        pacer.tick(|| {});
        assert!(pacer.next_deadline.is_some());
        pacer.resync();
        assert!(pacer.next_deadline.is_none());
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
        assert!(
            pacer.lagging,
            "a host slower than the target should be flagged"
        );
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
        assert!(
            pacer.lagging,
            "one on-time wake shouldn't clear the lag flag"
        );
        pacer.update_lag(false, t0 + Duration::from_secs(3));
        assert!(!pacer.lagging, "sustained on-time wakes clear it");
    }
}
