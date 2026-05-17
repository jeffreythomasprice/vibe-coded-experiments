//! Per-job progress tracker. The relay task in `worker_loop` feeds every
//! `ProgressEvent` here; the tracker accumulates known totals and per-phase
//! counters, then produces a `ProgressEnvelope` enriched with an overall
//! step count + ETA. The same envelope is what `status` reads back, so the
//! foreground spinner and `status` snapshot show identical wording.

use std::time::Instant;

use crate::protocol::{OverallProgress, ProgressEnvelope, ProgressEvent};

/// Weighted cost of one step in each phase. Reflects typical real-world
/// per-step duration (an LLM summary call dwarfs an embedding call which
/// dwarfs a chunking iteration). Used only for the rough overall progress
/// estimate — not load-bearing.
const W_PAGE: f64 = 1.0;
const W_OCR: f64 = 5.0;
const W_CHUNKING: f64 = 0.1;
const W_EMBED: f64 = 2.0;
const W_SUMMARY: f64 = 10.0;

/// Minimum count-bearing samples observed before we extrapolate an ETA.
/// Below this threshold the rate estimate is too noisy to be useful.
const ETA_MIN_SAMPLES: u64 = 3;

pub(crate) struct JobTracker {
    job_started_at: Instant,

    pages_total: Option<u32>,
    ocr_total: Option<u32>,
    chunks_total: Option<usize>,
    groups_per_level: Vec<usize>,

    pages_done: u32,
    ocr_done: u32,
    chunking_done: usize,
    embedding_done: usize,
    summaries_done: Vec<usize>,

    sample_count: u64,
    latest: Option<(ProgressEnvelope, Instant)>,
}

impl JobTracker {
    pub(crate) fn new(job_started_at: Instant) -> Self {
        Self {
            job_started_at,
            pages_total: None,
            ocr_total: None,
            chunks_total: None,
            groups_per_level: Vec::new(),
            pages_done: 0,
            ocr_done: 0,
            chunking_done: 0,
            embedding_done: 0,
            summaries_done: Vec::new(),
            sample_count: 0,
            latest: None,
        }
    }

    /// Update accumulated totals + per-phase counters from a single event.
    pub(crate) fn observe(&mut self, event: &ProgressEvent) {
        match event {
            ProgressEvent::Stage { .. } => {}
            ProgressEvent::Extracting { current, total } => {
                self.pages_total = Some(*total);
                self.pages_done = (*current).max(self.pages_done);
                self.sample_count += 1;
            }
            ProgressEvent::Ocr { current, total } => {
                self.ocr_total = Some(*total);
                self.ocr_done = (*current).max(self.ocr_done);
                self.sample_count += 1;
            }
            ProgressEvent::Chunking { current, total } => {
                self.chunks_total = Some(*total);
                self.chunking_done = (*current).max(self.chunking_done);
                self.sample_count += 1;
            }
            ProgressEvent::Embedding { current, total } => {
                self.chunks_total = Some(*total);
                self.embedding_done = (*current).max(self.embedding_done);
                self.sample_count += 1;
            }
            ProgressEvent::Summarizing {
                level,
                total_levels: _,
                current,
                total,
            } => {
                ensure_index(&mut self.groups_per_level, *level);
                ensure_index(&mut self.summaries_done, *level);
                self.groups_per_level[*level] = (*total).max(self.groups_per_level[*level]);
                self.summaries_done[*level] = (*current).max(self.summaries_done[*level]);
                self.sample_count += 1;
            }
        }
    }

    /// Build an envelope wrapping `event` with the current overall summary.
    /// Does not record it — call `set_latest` after.
    pub(crate) fn build_envelope(&self, event: ProgressEvent) -> ProgressEnvelope {
        let phase = phase_label(&event).to_string();
        let elapsed_secs = self.job_started_at.elapsed().as_secs();
        let elapsed_f = self.job_started_at.elapsed().as_secs_f64();

        let done = self.weighted_done();
        let total = self.weighted_total();

        let overall = if total > 0.0 {
            let eta_secs =
                if self.sample_count >= ETA_MIN_SAMPLES && done > 0.0 && total > done {
                    let secs_per_unit = elapsed_f / done;
                    let remaining = total - done;
                    Some((secs_per_unit * remaining).round() as u64)
                } else {
                    None
                };
            Some(OverallProgress {
                phase,
                step: done.round() as u64,
                total_steps: total.round() as u64,
                elapsed_secs,
                eta_secs,
            })
        } else {
            // No totals yet — still surface the phase + elapsed so the UI
            // has something to show during a pre-count stall.
            Some(OverallProgress {
                phase,
                step: 0,
                total_steps: 0,
                elapsed_secs,
                eta_secs: None,
            })
        };

        ProgressEnvelope {
            event,
            overall,
        }
    }

    pub(crate) fn set_latest(&mut self, envelope: ProgressEnvelope) {
        self.latest = Some((envelope, Instant::now()));
    }

    /// Snapshot accessor for the `status` handler: returns the latest
    /// enriched envelope plus how long ago it was recorded.
    pub(crate) fn latest(&self) -> (Option<ProgressEnvelope>, Option<u64>) {
        match &self.latest {
            Some((env, at)) => (Some(env.clone()), Some(at.elapsed().as_secs())),
            None => (None, None),
        }
    }

    fn weighted_done(&self) -> f64 {
        let mut sum = f64::from(self.pages_done) * W_PAGE
            + f64::from(self.ocr_done) * W_OCR
            + self.chunking_done as f64 * W_CHUNKING
            + self.embedding_done as f64 * W_EMBED;
        for &done in &self.summaries_done {
            sum += done as f64 * W_SUMMARY;
        }
        sum
    }

    fn weighted_total(&self) -> f64 {
        let mut sum = f64::from(self.pages_total.unwrap_or(0)) * W_PAGE
            + f64::from(self.ocr_total.unwrap_or(0)) * W_OCR;
        if let Some(c) = self.chunks_total {
            // Each chunk drives one chunking iteration and one embed call.
            sum += c as f64 * (W_CHUNKING + W_EMBED);
        }
        for &n in &self.groups_per_level {
            sum += n as f64 * W_SUMMARY;
        }
        sum
    }
}

fn phase_label(event: &ProgressEvent) -> &str {
    match event {
        ProgressEvent::Stage { name } => name.as_str(),
        ProgressEvent::Extracting { .. } => "extracting",
        ProgressEvent::Ocr { .. } => "ocr",
        ProgressEvent::Chunking { .. } => "chunking",
        ProgressEvent::Embedding { .. } => "embedding",
        ProgressEvent::Summarizing { .. } => "summarizing",
    }
}

fn ensure_index(v: &mut Vec<usize>, idx: usize) {
    while v.len() <= idx {
        v.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn extracting_then_chunking_grows_totals() {
        let mut t = JobTracker::new(Instant::now());
        t.observe(&ProgressEvent::Stage { name: "extracting".into() });
        t.observe(&ProgressEvent::Extracting { current: 1, total: 10 });
        t.observe(&ProgressEvent::Extracting { current: 5, total: 10 });
        let env = t.build_envelope(ProgressEvent::Extracting { current: 5, total: 10 });
        let o = env.overall.unwrap();
        assert_eq!(o.phase, "extracting");
        // Pages done so far: 5 × W_PAGE = 5.
        assert_eq!(o.step, 5);
        // Pages total: 10 × W_PAGE = 10.
        assert_eq!(o.total_steps, 10);
    }

    #[test]
    fn embedding_total_includes_chunking_and_embed_weights() {
        let mut t = JobTracker::new(Instant::now());
        t.observe(&ProgressEvent::Extracting { current: 10, total: 10 });
        t.observe(&ProgressEvent::Embedding { current: 1, total: 100 });
        let env = t.build_envelope(ProgressEvent::Embedding { current: 1, total: 100 });
        let o = env.overall.unwrap();
        // 10 pages × 1.0 = 10. 100 chunks × (0.1 + 2.0) = 210. Total = 220.
        assert_eq!(o.total_steps, 220);
        // Done: pages 10 × 1.0 + embed 1 × 2.0 = 12.
        assert_eq!(o.step, 12);
    }

    #[test]
    fn summarizing_appends_groups_per_level_to_total() {
        let mut t = JobTracker::new(Instant::now());
        t.observe(&ProgressEvent::Embedding { current: 60, total: 60 });
        t.observe(&ProgressEvent::Summarizing {
            level: 0,
            total_levels: 2,
            current: 1,
            total: 10,
        });
        let env = t.build_envelope(ProgressEvent::Summarizing {
            level: 0,
            total_levels: 2,
            current: 1,
            total: 10,
        });
        let o = env.overall.unwrap();
        // 60 chunks × 2.1 = 126. Level 0 groups: 10 × 10 = 100. Total = 226.
        assert_eq!(o.total_steps, 226);
        // Done: chunking_done is still 0 (no Chunking events observed), only
        // embedding_done = 60. So 60 × 2.0 (embed) + 1 × 10 (one summary) = 130.
        assert_eq!(o.step, 130);
    }

    #[test]
    fn eta_is_none_until_min_samples() {
        let mut t = JobTracker::new(Instant::now());
        t.observe(&ProgressEvent::Extracting { current: 1, total: 100 });
        let env = t.build_envelope(ProgressEvent::Extracting { current: 1, total: 100 });
        assert!(env.overall.unwrap().eta_secs.is_none());
    }

    #[test]
    fn eta_present_after_min_samples_and_elapsed_time() {
        let mut t = JobTracker::new(Instant::now());
        // Burn a measurable amount of elapsed time so the rate computation
        // has a non-zero denominator.
        sleep(Duration::from_millis(50));
        t.observe(&ProgressEvent::Extracting { current: 1, total: 100 });
        t.observe(&ProgressEvent::Extracting { current: 2, total: 100 });
        t.observe(&ProgressEvent::Extracting { current: 3, total: 100 });
        let env = t.build_envelope(ProgressEvent::Extracting { current: 3, total: 100 });
        assert!(env.overall.unwrap().eta_secs.is_some());
    }

    #[test]
    fn stage_event_with_no_counts_still_surfaces_phase() {
        let t = JobTracker::new(Instant::now());
        let env = t.build_envelope(ProgressEvent::Stage { name: "summarizing".into() });
        let o = env.overall.unwrap();
        assert_eq!(o.phase, "summarizing");
        assert_eq!(o.total_steps, 0);
        assert!(o.eta_secs.is_none());
    }

    #[test]
    fn observe_is_monotonic_for_current_count() {
        let mut t = JobTracker::new(Instant::now());
        t.observe(&ProgressEvent::Embedding { current: 50, total: 60 });
        // A stale lower current shouldn't roll back the counter.
        t.observe(&ProgressEvent::Embedding { current: 49, total: 60 });
        let env = t.build_envelope(ProgressEvent::Embedding { current: 49, total: 60 });
        // 60 chunks × 2.1 = 126 total. 50 × 2.0 = 100 done (not 49 × 2.0).
        let o = env.overall.unwrap();
        assert_eq!(o.total_steps, 126);
        assert_eq!(o.step, 100);
    }
}
