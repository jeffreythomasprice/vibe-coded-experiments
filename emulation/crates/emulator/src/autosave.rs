//! When the running game is due to be autosaved. This only tracks time and
//! progress; writing the state is the app's job.

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Autosave {
    /// `None` turns periodic autosaving off.
    interval: Option<Duration>,
    /// When the timer last restarted.
    since: Instant,
    /// Whether any frame has run since then.
    unsaved: bool,
}

impl Autosave {
    pub fn new(interval: Option<Duration>, now: Instant) -> Autosave {
        Autosave {
            interval,
            since: now,
            unsaved: false,
        }
    }

    /// Note that the machine has advanced.
    pub fn frame_ran(&mut self) {
        self.unsaved = true;
    }

    /// Whether the machine has advanced since the last [`Autosave::reset`], so
    /// there is something an autosave would add.
    pub fn has_unsaved_progress(&self) -> bool {
        self.unsaved
    }

    /// Whether a periodic autosave should happen now.
    pub fn is_due(&self, now: Instant) -> bool {
        self.unsaved
            && self
                .interval
                .is_some_and(|interval| now.saturating_duration_since(self.since) >= interval)
    }

    /// Start over: after an autosave, or when the machine is replaced.
    pub fn reset(&mut self, now: Instant) {
        self.since = now;
        self.unsaved = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn due_once_the_interval_passes_with_progress() {
        let start = Instant::now();
        let mut autosave = Autosave::new(Some(MINUTE), start);
        // Nothing has run, however long it has been.
        assert!(!autosave.is_due(start + MINUTE * 5));

        autosave.frame_ran();
        assert!(!autosave.is_due(start + MINUTE / 2));
        assert!(autosave.is_due(start + MINUTE));
    }

    #[test]
    fn reset_restarts_the_timer_and_clears_progress() {
        let start = Instant::now();
        let mut autosave = Autosave::new(Some(MINUTE), start);
        autosave.frame_ran();
        autosave.reset(start + MINUTE);
        assert!(!autosave.has_unsaved_progress());

        autosave.frame_ran();
        assert!(!autosave.is_due(start + MINUTE * 3 / 2));
        assert!(autosave.is_due(start + MINUTE * 2));
    }

    #[test]
    fn without_an_interval_it_is_never_due_but_still_tracks_progress() {
        let start = Instant::now();
        let mut autosave = Autosave::new(None, start);
        autosave.frame_ran();
        assert!(autosave.has_unsaved_progress());
        assert!(!autosave.is_due(start + MINUTE * 1000));
    }
}
