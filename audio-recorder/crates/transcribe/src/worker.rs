use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use audio_recorder_core::{
    ResolvedConfig, compress,
    compress::CompressError,
    config::{CompressionSettings, LlmSettings, SummarizationSettings, TranscriptionSettings},
    jobs::{JobError, JobStore},
    library, summary,
    summary::SummaryError,
    transcript,
};
use audio_recorder_schema::jobs::{Job, JobKind, JobState};

use crate::{Progress, ProgressFn, TranscribeError, transcribe_session};

const SAVE_INTERVAL: Duration = Duration::from_millis(500);
const KEEP_FINISHED: usize = 20;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

pub trait Runner: Sync {
    fn run(&self, job: &Job, progress: ProgressFn, cancel: Arc<AtomicBool>) -> Result<(), RunError>;

    fn follow_ups(&self, _job: &Job) -> Vec<(JobKind, bool)> {
        Vec::new()
    }
}

pub struct JobRunner {
    pub sessions_dir: PathBuf,
    pub transcription: TranscriptionSettings,
    pub compression: CompressionSettings,
    pub llm: LlmSettings,
    pub summarization: SummarizationSettings,
}

impl JobRunner {
    pub fn new(config: &ResolvedConfig) -> Self {
        Self {
            sessions_dir: config.sessions_dir.clone(),
            transcription: config.transcription.clone(),
            compression: config.compression,
            llm: config.llm.clone(),
            summarization: config.summarization.clone(),
        }
    }

    fn transcribe(&self, job: &Job, progress: ProgressFn, cancel: Arc<AtomicBool>) -> Result<(), RunError> {
        let dir = library::session_dir(&self.sessions_dir, &job.session).map_err(|e| RunError::Failed(e.to_string()))?;
        if !job.replace && transcript::exists(&dir) {
            return Err(RunError::Failed("a transcript already exists; retranscribe to replace it".into()));
        }
        match transcribe_session(&dir, &self.transcription, progress, cancel) {
            Ok(_) => Ok(()),
            Err(TranscribeError::Cancelled) => Err(RunError::Cancelled),
            Err(e) => Err(RunError::Failed(e.to_string())),
        }
    }

    fn compress(&self, job: &Job, progress: ProgressFn, cancel: Arc<AtomicBool>) -> Result<(), RunError> {
        let dir = library::session_dir(&self.sessions_dir, &job.session).map_err(|e| RunError::Failed(e.to_string()))?;
        let report = |fraction: f32| progress(Progress { stage: "Compressing".into(), fraction: Some(fraction) });
        match compress::compress_session(&dir, &self.compression, &report, &cancel) {
            Ok(summary) => {
                tracing::info!(
                    "compressed {} segments of {}: {} -> {} bytes",
                    summary.segments,
                    job.session,
                    summary.bytes_before,
                    summary.bytes_after
                );
                Ok(())
            }
            Err(CompressError::Cancelled) => Err(RunError::Cancelled),
            Err(e) => Err(RunError::Failed(e.to_string())),
        }
    }

    fn summarize(&self, job: &Job, progress: ProgressFn, cancel: Arc<AtomicBool>) -> Result<(), RunError> {
        let dir = library::session_dir(&self.sessions_dir, &job.session).map_err(|e| RunError::Failed(e.to_string()))?;
        if !job.replace && summary::exists(&dir) {
            return Err(RunError::Failed("a summary already exists; re-summarize to replace it".into()));
        }
        let report = |stage: &str, fraction: Option<f32>| progress(Progress { stage: stage.to_string(), fraction });
        match summary::summarize_session(&dir, &self.llm, &self.summarization, &report, &cancel) {
            Ok(()) => Ok(()),
            Err(SummaryError::Cancelled) => Err(RunError::Cancelled),
            Err(e) => Err(RunError::Failed(e.to_string())),
        }
    }
}

impl Runner for JobRunner {
    fn run(&self, job: &Job, progress: ProgressFn, cancel: Arc<AtomicBool>) -> Result<(), RunError> {
        match job.kind {
            JobKind::Transcribe => self.transcribe(job, progress, cancel),
            JobKind::Compress => self.compress(job, progress, cancel),
            JobKind::Summarize => self.summarize(job, progress, cancel),
        }
    }

    fn follow_ups(&self, job: &Job) -> Vec<(JobKind, bool)> {
        if !matches!(job.kind, JobKind::Transcribe) || !self.summarization.auto {
            return Vec::new();
        }
        let has_speech = library::session_dir(&self.sessions_dir, &job.session)
            .ok()
            .and_then(|dir| transcript::load(&dir).ok().flatten())
            .is_some_and(|t| !t.snippets.is_empty());
        if has_speech { vec![(JobKind::Summarize, true)] } else { Vec::new() }
    }
}

pub fn run_until_idle(store: &JobStore, runner: &dyn Runner, stop: &AtomicBool) -> Result<Option<usize>, JobError> {
    let Some(lock) = store.try_lock_worker()? else { return Ok(None) };
    store.requeue_stale(&lock)?;
    let mut processed = 0;
    while !stop.load(Ordering::Relaxed) {
        let Some(job) = store.claim_next()? else { break };
        run_one(store, runner, job, stop)?;
        processed += 1;
        store.prune_finished(KEEP_FINISHED)?;
    }
    Ok(Some(processed))
}

fn run_one(store: &JobStore, runner: &dyn Runner, job: Job, stop: &AtomicBool) -> Result<(), JobError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let done = AtomicBool::new(false);
    let shared = Arc::new(Mutex::new((job.clone(), Instant::now() - SAVE_INTERVAL)));
    let progress: ProgressFn = {
        let shared = shared.clone();
        let store = store.clone();
        Arc::new(move |p: Progress| {
            let mut guard = shared.lock().expect("progress lock");
            let (job, last) = &mut *guard;
            let changed = job.stage.as_deref() != Some(p.stage.as_str());
            if !changed && last.elapsed() < SAVE_INTERVAL {
                return;
            }
            job.stage = Some(p.stage);
            job.fraction = p.fraction.map(f64::from);
            *last = Instant::now();
            if let Err(e) = store.save(job) {
                tracing::warn!("could not save job progress: {e}");
            }
        })
    };

    let outcome = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if store.cancel_requested(&job.id) || stop.load(Ordering::Relaxed) {
                    cancel.store(true, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.run(&job, progress, cancel.clone())))
            .unwrap_or_else(|_| Err(RunError::Failed("internal error (see log)".into())));
        done.store(true, Ordering::Relaxed);
        outcome
    });

    let mut job = shared.lock().expect("progress lock").0.clone();
    match outcome {
        Ok(()) => {
            store.finish(&mut job, JobState::Done, None)?;
            for (kind, replace) in runner.follow_ups(&job) {
                if let Err(e) = store.enqueue(kind, &job.session, replace) {
                    tracing::warn!("could not queue a follow-up task for {}: {e}", job.session);
                }
            }
            Ok(())
        }
        Err(RunError::Cancelled) if stop.load(Ordering::Relaxed) && !store.cancel_requested(&job.id) => {
            job.state = JobState::Queued;
            job.started_at = None;
            job.stage = None;
            job.fraction = None;
            store.save(&job)
        }
        Err(RunError::Cancelled) => store.finish(&mut job, JobState::Cancelled, None),
        Err(RunError::Failed(message)) => {
            tracing::error!("job {} failed: {message}", job.id);
            store.finish(&mut job, JobState::Failed, Some(message))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    struct Stub {
        order: Mutex<Vec<String>>,
        fail: &'static str,
        hang: &'static str,
        calls: AtomicUsize,
    }

    impl Runner for Stub {
        fn run(&self, job: &Job, progress: ProgressFn, cancel: Arc<AtomicBool>) -> Result<(), RunError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.order.lock().unwrap().push(job.session.clone());
            progress(Progress { stage: "working".into(), fraction: Some(0.5) });
            if job.session == self.hang {
                while !cancel.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                return Err(RunError::Cancelled);
            }
            if job.session == self.fail {
                return Err(RunError::Failed("boom".into()));
            }
            Ok(())
        }
    }

    fn store(tag: &str) -> JobStore {
        let dir = std::env::temp_dir().join(format!("audio-recorder-worker-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        JobStore::at(dir)
    }

    fn stub(fail: &'static str, hang: &'static str) -> Stub {
        Stub { order: Mutex::new(Vec::new()), fail, hang, calls: AtomicUsize::new(0) }
    }

    #[test]
    fn runs_jobs_in_order_and_records_outcomes() {
        let s = store("order");
        for name in ["a", "b", "c"] {
            s.enqueue(JobKind::Transcribe, name, false).unwrap();
        }
        let runner = stub("b", "");
        let stop = AtomicBool::new(false);
        assert_eq!(run_until_idle(&s, &runner, &stop).unwrap(), Some(3));
        assert_eq!(*runner.order.lock().unwrap(), ["a", "b", "c"]);
        let states: Vec<_> = s.list().unwrap().into_iter().map(|j| (j.session, j.state, j.error)).collect();
        assert_eq!(states[0].1, JobState::Done);
        assert_eq!(states[1], ("b".to_string(), JobState::Failed, Some("boom".to_string())));
        assert_eq!(states[2].1, JobState::Done);
    }

    #[test]
    fn a_finished_job_queues_its_follow_ups_which_run_in_the_same_pass() {
        struct Chained(Mutex<Vec<(JobKind, bool)>>);
        impl Runner for Chained {
            fn run(&self, job: &Job, _: ProgressFn, _: Arc<AtomicBool>) -> Result<(), RunError> {
                self.0.lock().unwrap().push((job.kind, job.replace));
                if job.session == "bad" { Err(RunError::Failed("boom".into())) } else { Ok(()) }
            }
            fn follow_ups(&self, job: &Job) -> Vec<(JobKind, bool)> {
                if matches!(job.kind, JobKind::Transcribe) { vec![(JobKind::Summarize, true)] } else { Vec::new() }
            }
        }
        let s = store("followups");
        s.enqueue(JobKind::Transcribe, "good", false).unwrap();
        s.enqueue(JobKind::Transcribe, "bad", false).unwrap();
        let runner = Chained(Mutex::new(Vec::new()));
        assert_eq!(run_until_idle(&s, &runner, &AtomicBool::new(false)).unwrap(), Some(3));
        assert_eq!(
            *runner.0.lock().unwrap(),
            [(JobKind::Transcribe, false), (JobKind::Transcribe, false), (JobKind::Summarize, true)]
        );
        let jobs = s.list().unwrap();
        assert_eq!(jobs.iter().filter(|j| matches!(j.kind, JobKind::Summarize)).count(), 1);
    }

    #[test]
    fn a_panicking_runner_fails_the_job_but_not_the_worker() {
        struct Panics;
        impl Runner for Panics {
            fn run(&self, job: &Job, _: ProgressFn, _: Arc<AtomicBool>) -> Result<(), RunError> {
                if job.session == "a" {
                    panic!("boom");
                }
                Ok(())
            }
        }
        let s = store("panic");
        s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        s.enqueue(JobKind::Transcribe, "b", false).unwrap();
        assert_eq!(run_until_idle(&s, &Panics, &AtomicBool::new(false)).unwrap(), Some(2));
        let jobs = s.list().unwrap();
        assert_eq!(jobs[0].state, JobState::Failed);
        assert_eq!(jobs[1].state, JobState::Done);
    }

    #[test]
    fn second_worker_exits_while_first_holds_the_lock() {
        let s = store("lock");
        s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        let _held = s.try_lock_worker().unwrap().unwrap();
        let runner = stub("", "");
        assert_eq!(run_until_idle(&s, &runner, &AtomicBool::new(false)).unwrap(), None);
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancel_marker_stops_a_running_job() {
        let s = store("cancel");
        let (job, _) = s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        let runner = stub("", "a");
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while runner.calls.load(Ordering::SeqCst) == 0 {
                    std::thread::sleep(Duration::from_millis(20));
                }
                s.cancel(&job.id).unwrap();
            });
            run_until_idle(&s, &runner, &stop).unwrap();
        });
        assert_eq!(s.get(&job.id).unwrap().state, JobState::Cancelled);
    }

    #[test]
    fn stop_requeues_the_running_job() {
        let s = store("stop");
        let (job, _) = s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        let runner = stub("", "a");
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while runner.calls.load(Ordering::SeqCst) == 0 {
                    std::thread::sleep(Duration::from_millis(20));
                }
                stop.store(true, Ordering::Relaxed);
            });
            run_until_idle(&s, &runner, &stop).unwrap();
        });
        assert_eq!(s.get(&job.id).unwrap().state, JobState::Queued);
    }
}
