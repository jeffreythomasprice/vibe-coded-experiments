//! Per-job progress tracker. The relay task in `worker_loop` feeds every
//! `ProgressEvent` here; the tracker accumulates known totals and per-phase
//! counters, then produces a `ProgressEnvelope` enriched with an overall
//! step count + ETA. Envelopes are rebuilt on demand (relay heartbeat,
//! `status` snapshot) so elapsed / ETA / idle stay live between events, and
//! the foreground spinner and `status` snapshot show identical wording.

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

/// What the job will do after embedding, so the summary tree's cost can be
/// counted in the overall total before the summarize phase actually starts.
/// `None` on the tracker means "no summary phase expected".
#[derive(Debug, Clone, Copy)]
pub(crate) struct SummaryPlan {
    pub group_size: usize,
    pub max_depth: usize,
    /// Expected number of per-language trees (see
    /// `language::estimate_summary_language_count`). Superseded by the
    /// `trees` field of the first `Summarizing` event.
    pub languages: usize,
}

/// Pipeline order. Seeing an event from a later phase implies every earlier
/// phase is finished, which lets us close out counters whose final event
/// reports "working on N" rather than "finished N".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Extracting,
    Ocr,
    Chunking,
    Embedding,
    Summarizing,
}

/// Rate measurement for the phase currently running. ETA prefers this over
/// the whole-job average, since early phases (pdftotext) are orders of
/// magnitude cheaper per weighted unit than late ones (LLM calls) and would
/// otherwise drag the estimate way down.
struct PhaseClock {
    phase: Phase,
    started_at: Instant,
    done_at_start: f64,
    samples: u64,
}

pub(crate) struct JobTracker {
    job_started_at: Instant,
    summary_plan: Option<SummaryPlan>,

    pages_total: Option<u32>,
    ocr_total: Option<u32>,
    chunks_total: Option<usize>,
    /// Indexed `[tree][level]`; one tree per summary language.
    groups_per_level: Vec<Vec<usize>>,
    /// Tree count reported by summarize itself, once it has started.
    trees_observed: Option<usize>,

    pages_done: u32,
    ocr_done: u32,
    chunking_done: usize,
    embedding_done: usize,
    /// Indexed `[tree][level]`, parallel to `groups_per_level`.
    summaries_done: Vec<Vec<usize>>,

    sample_count: u64,
    phase_clock: Option<PhaseClock>,
    latest: Option<(ProgressEvent, Instant)>,
}

impl JobTracker {
    pub(crate) fn new(job_started_at: Instant, summary_plan: Option<SummaryPlan>) -> Self {
        Self {
            job_started_at,
            summary_plan,
            pages_total: None,
            ocr_total: None,
            chunks_total: None,
            groups_per_level: Vec::new(),
            trees_observed: None,
            pages_done: 0,
            ocr_done: 0,
            chunking_done: 0,
            embedding_done: 0,
            summaries_done: Vec::new(),
            sample_count: 0,
            phase_clock: None,
            latest: None,
        }
    }

    /// Observe `event`, remember it as the latest, and return the enriched
    /// envelope to forward to the client.
    pub(crate) fn record(&mut self, event: ProgressEvent) -> ProgressEnvelope {
        let now = Instant::now();
        self.observe_at(&event, now);
        self.latest = Some((event.clone(), now));
        self.envelope_at(event, now)
    }

    /// Latest event re-enriched against the current clock, plus seconds since
    /// it was recorded. Used by the `status` snapshot and the relay heartbeat.
    pub(crate) fn latest(&self) -> (Option<ProgressEnvelope>, Option<u64>) {
        match &self.latest {
            Some((event, at)) => (
                Some(self.envelope_at(event.clone(), Instant::now())),
                Some(at.elapsed().as_secs()),
            ),
            None => (None, None),
        }
    }

    /// Update accumulated totals + per-phase counters from a single event.
    ///
    /// Note the "done" semantics: Extracting and Chunking are emitted after
    /// the unit finishes, but Ocr, Embedding, and Summarizing are emitted
    /// *before* starting unit `current`, so only `current - 1` are done.
    fn observe_at(&mut self, event: &ProgressEvent, now: Instant) {
        let phase = match event {
            ProgressEvent::Stage { .. } => return,
            ProgressEvent::Extracting { current, total } => {
                self.pages_total = Some(*total);
                self.pages_done = (*current).max(self.pages_done);
                Phase::Extracting
            }
            ProgressEvent::Ocr { current, total } => {
                self.ocr_total = Some(*total);
                self.ocr_done = current.saturating_sub(1).max(self.ocr_done);
                Phase::Ocr
            }
            ProgressEvent::Chunking { current, total } => {
                self.chunks_total = Some(*total);
                self.chunking_done = (*current).max(self.chunking_done);
                Phase::Chunking
            }
            ProgressEvent::Embedding { current, total } => {
                self.chunks_total = Some(*total);
                self.embedding_done = current.saturating_sub(1).max(self.embedding_done);
                Phase::Embedding
            }
            ProgressEvent::Summarizing {
                level,
                total_levels: _,
                current,
                total,
                tree,
                trees,
                language: _,
            } => {
                let (tree, level) = (*tree, *level);
                self.trees_observed = Some(*trees);
                while self.groups_per_level.len() <= tree {
                    self.groups_per_level.push(Vec::new());
                    self.summaries_done.push(Vec::new());
                }
                let groups = &mut self.groups_per_level[tree];
                let done = &mut self.summaries_done[tree];
                ensure_index(groups, level);
                ensure_index(done, level);
                groups[level] = (*total).max(groups[level]);
                done[level] = current.saturating_sub(1).max(done[level]);
                // Moving to level k means every lower level of this tree
                // finished.
                done[..level].copy_from_slice(&groups[..level]);
                // Moving to tree t means every earlier tree finished.
                for t in 0..tree {
                    let finished = self.groups_per_level[t].clone();
                    self.summaries_done[t] = finished;
                }
                Phase::Summarizing
            }
        };
        self.complete_phases_before(phase);
        self.sample_count += 1;

        match &mut self.phase_clock {
            Some(clock) if clock.phase == phase => clock.samples += 1,
            _ => {
                self.phase_clock = Some(PhaseClock {
                    phase,
                    started_at: now,
                    done_at_start: self.weighted_done(),
                    samples: 1,
                });
            }
        }
    }

    fn complete_phases_before(&mut self, phase: Phase) {
        if phase > Phase::Extracting
            && let Some(t) = self.pages_total
        {
            self.pages_done = t;
        }
        if phase > Phase::Ocr
            && let Some(t) = self.ocr_total
        {
            self.ocr_done = t;
        }
        if phase > Phase::Chunking
            && let Some(t) = self.chunks_total
        {
            self.chunking_done = t;
        }
        if phase > Phase::Embedding
            && let Some(t) = self.chunks_total
        {
            self.embedding_done = t;
        }
    }

    fn envelope_at(&self, event: ProgressEvent, now: Instant) -> ProgressEnvelope {
        let phase = phase_label(&event).to_string();
        let elapsed_secs = now.saturating_duration_since(self.job_started_at).as_secs();
        let idle_secs = self
            .latest
            .as_ref()
            .map(|(_, at)| now.saturating_duration_since(*at).as_secs());

        let done = self.weighted_done();
        let total = self.weighted_total().max(done);

        ProgressEnvelope {
            event,
            // Even with no totals yet we still surface the phase + elapsed so
            // the UI has something to show during a pre-count stall.
            overall: Some(OverallProgress {
                phase,
                step: done.round() as u64,
                total_steps: total.round() as u64,
                elapsed_secs,
                eta_secs: self.eta_secs(done, total, now),
                idle_secs,
            }),
        }
    }

    fn eta_secs(&self, done: f64, total: f64, now: Instant) -> Option<u64> {
        if total <= done || self.sample_count < ETA_MIN_SAMPLES {
            return None;
        }
        let phase_rate = self.phase_clock.as_ref().and_then(|c| {
            let d = done - c.done_at_start;
            let t = now.saturating_duration_since(c.started_at).as_secs_f64();
            (c.samples >= ETA_MIN_SAMPLES && d > 0.0 && t > 0.0).then(|| t / d)
        });
        let global_rate = || {
            let t = now.saturating_duration_since(self.job_started_at).as_secs_f64();
            (done > 0.0 && t > 0.0).then(|| t / done)
        };
        let secs_per_unit = phase_rate.or_else(global_rate)?;
        Some((secs_per_unit * (total - done)).round() as u64)
    }

    fn weighted_done(&self) -> f64 {
        let mut sum = f64::from(self.pages_done) * W_PAGE
            + f64::from(self.ocr_done) * W_OCR
            + self.chunking_done as f64 * W_CHUNKING
            + self.embedding_done as f64 * W_EMBED;
        for &done in self.summaries_done.iter().flatten() {
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
        for tree in self.summary_groups_per_tree() {
            for n in tree {
                sum += n as f64 * W_SUMMARY;
            }
        }
        sum
    }

    /// Group counts per tree per summary level: observed where summarize
    /// has reached that level, otherwise projected from the level below
    /// (mirrors the `group_consecutive` / `inputs.len() <= 1` logic in
    /// `summarize`). Every tree is built over the same chunks, so a tree
    /// that hasn't started borrows tree 0's observed shape.
    fn summary_groups_per_tree(&self) -> Vec<Vec<usize>> {
        let mut out = self.groups_per_level.clone();
        let (Some(plan), Some(chunks)) = (self.summary_plan, self.chunks_total) else {
            return out;
        };
        let trees = self.trees_observed.unwrap_or(plan.languages).max(1);
        while out.len() < trees {
            out.push(Vec::new());
        }
        let template = self.groups_per_level.first().cloned().unwrap_or_default();
        let g = plan.group_size.max(2);
        for tree in out.iter_mut().take(trees) {
            let mut n = chunks;
            for level in 0..plan.max_depth {
                if n <= 1 {
                    break;
                }
                let count = match (tree.get(level), template.get(level)) {
                    (Some(&observed), _) if observed > 0 => observed,
                    (_, Some(&shared)) if shared > 0 => shared,
                    _ => n.div_ceil(g),
                };
                ensure_index(tree, level);
                tree[level] = count;
                n = count;
            }
        }
        out
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
    use std::time::Duration;

    fn summarizing(level: usize, current: usize, total: usize) -> ProgressEvent {
        summarizing_tree(0, 1, level, current, total)
    }

    fn summarizing_tree(
        tree: usize,
        trees: usize,
        level: usize,
        current: usize,
        total: usize,
    ) -> ProgressEvent {
        ProgressEvent::Summarizing {
            level,
            total_levels: 3,
            current,
            total,
            tree,
            trees,
            language: None,
        }
    }

    #[test]
    fn extracting_then_chunking_grows_totals() {
        let mut t = JobTracker::new(Instant::now(), None);
        t.record(ProgressEvent::Stage { name: "extracting".into() });
        t.record(ProgressEvent::Extracting { current: 1, total: 10 });
        let o = t
            .record(ProgressEvent::Extracting { current: 5, total: 10 })
            .overall
            .unwrap();
        assert_eq!(o.phase, "extracting");
        // Pages done so far: 5 × W_PAGE = 5. Total: 10 × W_PAGE = 10.
        assert_eq!(o.step, 5);
        assert_eq!(o.total_steps, 10);
    }

    #[test]
    fn embedding_total_includes_chunking_and_embed_weights() {
        let mut t = JobTracker::new(Instant::now(), None);
        t.record(ProgressEvent::Extracting { current: 10, total: 10 });
        let o = t
            .record(ProgressEvent::Embedding { current: 2, total: 100 })
            .overall
            .unwrap();
        // 10 pages × 1.0 = 10. 100 chunks × (0.1 + 2.0) = 210. Total = 220.
        assert_eq!(o.total_steps, 220);
        // Done: pages 10 + chunking implicitly complete 100 × 0.1 = 10
        // + one embed finished (working on #2) × 2.0 = 22.
        assert_eq!(o.step, 22);
    }

    #[test]
    fn summary_plan_projects_tree_cost_before_summarize_starts() {
        let plan = SummaryPlan { group_size: 6, max_depth: 4, languages: 1 };
        let mut t = JobTracker::new(Instant::now(), Some(plan));
        let o = t
            .record(ProgressEvent::Embedding { current: 1, total: 77 })
            .overall
            .unwrap();
        // 77 chunks × 2.1 = 161.7. Tree: 13 + 3 + 1 = 17 summaries × 10 = 170.
        assert_eq!(o.total_steps, 332);
    }

    #[test]
    fn summarizing_marks_embedding_and_lower_levels_complete() {
        let plan = SummaryPlan { group_size: 6, max_depth: 4, languages: 1 };
        let mut t = JobTracker::new(Instant::now(), Some(plan));
        t.record(ProgressEvent::Embedding { current: 77, total: 77 });
        t.record(summarizing(0, 13, 13));
        let o = t.record(summarizing(1, 1, 3)).overall.unwrap();
        // Done: 77 × 2.1 = 161.7, level 0 fully done 13 × 10 = 130. = 292.
        assert_eq!(o.step, 292);
        // Total unchanged from projection: 161.7 + 170 = 332.
        assert_eq!(o.total_steps, 332);
    }

    #[test]
    fn observed_level_count_overrides_projection() {
        let plan = SummaryPlan { group_size: 6, max_depth: 4, languages: 1 };
        let mut t = JobTracker::new(Instant::now(), Some(plan));
        t.record(ProgressEvent::Embedding { current: 1, total: 77 });
        // Pretend summarize found 20 groups at level 0 (e.g. config changed).
        let o = t.record(summarizing(0, 1, 20)).overall.unwrap();
        // Tree: 20 + ceil(20/6)=4 + 1 = 25 summaries × 10 = 250.
        assert_eq!(o.total_steps, 162 + 250);
    }

    #[test]
    fn eta_is_none_until_min_samples() {
        let mut t = JobTracker::new(Instant::now(), None);
        let env = t.record(ProgressEvent::Extracting { current: 1, total: 100 });
        assert!(env.overall.unwrap().eta_secs.is_none());
    }

    #[test]
    fn eta_uses_current_phase_rate() {
        let start = Instant::now();
        let mut t = JobTracker::new(start, None);
        // Extraction is instant (all at t=0)…
        t.observe_at(&ProgressEvent::Extracting { current: 100, total: 100 }, start);
        // …then embedding takes 1s per chunk.
        for i in 1..=11 {
            let at = start + Duration::from_secs(i as u64 - 1);
            t.observe_at(&ProgressEvent::Embedding { current: i, total: 50 }, at);
        }
        let now = start + Duration::from_secs(10);
        let o = t
            .envelope_at(ProgressEvent::Embedding { current: 11, total: 50 }, now)
            .overall
            .unwrap();
        // Phase clock: 10 embeds (20 units) in 10s → 0.5 s/unit. Remaining:
        // 40 embeds × 2.0 = 80 units → 40s. The whole-job average would have
        // been diluted by the 100 free pages.
        assert_eq!(o.eta_secs, Some(40));
    }

    #[test]
    fn latest_reports_live_elapsed_and_idle() {
        let start = Instant::now() - Duration::from_secs(100);
        let mut t = JobTracker::new(start, None);
        t.record(ProgressEvent::Stage { name: "summarizing".into() });
        let (env, age) = t.latest();
        let o = env.unwrap().overall.unwrap();
        assert!(o.elapsed_secs >= 100);
        assert_eq!(age, Some(0));
        assert!(o.idle_secs.unwrap() <= 1);
    }

    #[test]
    fn fresh_event_reports_zero_idle() {
        let mut t = JobTracker::new(Instant::now(), None);
        t.latest = Some((
            ProgressEvent::Stage { name: "extracting".into() },
            Instant::now() - Duration::from_secs(90),
        ));
        let o = t
            .record(ProgressEvent::Extracting { current: 1, total: 2 })
            .overall
            .unwrap();
        assert_eq!(o.idle_secs, Some(0));
    }

    #[test]
    fn stage_event_with_no_counts_still_surfaces_phase() {
        let mut t = JobTracker::new(Instant::now(), None);
        let o = t
            .record(ProgressEvent::Stage { name: "summarizing".into() })
            .overall
            .unwrap();
        assert_eq!(o.phase, "summarizing");
        assert_eq!(o.total_steps, 0);
        assert!(o.eta_secs.is_none());
    }

    #[test]
    fn observe_is_monotonic_for_current_count() {
        let mut t = JobTracker::new(Instant::now(), None);
        t.record(ProgressEvent::Embedding { current: 51, total: 60 });
        // A stale lower current shouldn't roll back the counter.
        let o = t
            .record(ProgressEvent::Embedding { current: 49, total: 60 })
            .overall
            .unwrap();
        // 60 chunks × 2.1 = 126 total. Chunking complete 6 + 50 × 2.0 = 106.
        assert_eq!(o.total_steps, 126);
        assert_eq!(o.step, 106);
    }

    #[test]
    fn two_language_plan_doubles_projected_tree_cost() {
        let plan = SummaryPlan { group_size: 6, max_depth: 4, languages: 2 };
        let mut t = JobTracker::new(Instant::now(), Some(plan));
        let o = t
            .record(ProgressEvent::Embedding { current: 1, total: 77 })
            .overall
            .unwrap();
        // 161.7 + 2 trees × 17 summaries × 10 = 501.7.
        assert_eq!(o.total_steps, 502);
    }

    #[test]
    fn second_tree_marks_first_tree_complete() {
        let plan = SummaryPlan { group_size: 6, max_depth: 4, languages: 2 };
        let mut t = JobTracker::new(Instant::now(), Some(plan));
        t.record(ProgressEvent::Embedding { current: 77, total: 77 });
        t.record(summarizing_tree(0, 2, 0, 13, 13));
        t.record(summarizing_tree(0, 2, 1, 3, 3));
        t.record(summarizing_tree(0, 2, 2, 1, 1));
        let o = t.record(summarizing_tree(1, 2, 0, 1, 13)).overall.unwrap();
        // Done: 161.7 + tree 0 all 17 × 10 = 331.7.
        assert_eq!(o.step, 332);
        assert_eq!(o.total_steps, 502);
    }

    #[test]
    fn observed_tree_count_overrides_plan() {
        // Plan guessed 2 trees ("en" + "source"), but the source turned out
        // to be English so summarize only builds one.
        let plan = SummaryPlan { group_size: 6, max_depth: 4, languages: 2 };
        let mut t = JobTracker::new(Instant::now(), Some(plan));
        t.record(ProgressEvent::Embedding { current: 77, total: 77 });
        let o = t.record(summarizing_tree(0, 1, 0, 1, 13)).overall.unwrap();
        assert_eq!(o.total_steps, 332);
    }
}
