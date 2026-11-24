use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use audio_recorder_schema::{
    SchemaError, from_json_str,
    jobs::{Job, JobKind, JobState},
    to_json_string_pretty,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description};

use crate::{
    atomic,
    paths::{self, PathsError},
};

const LOCK_FILE: &str = "worker.lock";
const JOB_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error(transparent)]
    Paths(#[from] PathsError),
    #[error("job storage error at {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("invalid job file {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error("could not serialize job: {0}")]
    Serialize(#[from] SchemaError),
    #[error("no such job: {0}")]
    NotFound(String),
}

pub fn is_active(state: &JobState) -> bool {
    matches!(state, JobState::Queued | JobState::Running)
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

fn timestamp() -> String {
    now().format(&Rfc3339).unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct JobStore {
    dir: PathBuf,
}

pub struct WorkerLock {
    _file: File,
}

impl JobStore {
    pub fn open_default() -> Result<Self, JobError> {
        Ok(Self::at(paths::jobs_dir()?))
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn io<T>(path: &Path, r: io::Result<T>) -> Result<T, JobError> {
        r.map_err(|source| JobError::Io { path: path.to_path_buf(), source })
    }

    fn job_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn cancel_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.cancel"))
    }

    fn new_id(&self) -> String {
        let t = now();
        let base = t.format(format_description!("[year][month][day]T[hour][minute][second]")).unwrap_or_default();
        let mut nanos = t.nanosecond();
        loop {
            let id = format!("{base}-{nanos:09}");
            if !self.job_path(&id).exists() {
                return id;
            }
            nanos += 1;
        }
    }

    pub fn try_lock_worker(&self) -> Result<Option<WorkerLock>, JobError> {
        Self::io(&self.dir, fs::create_dir_all(&self.dir))?;
        let path = self.dir.join(LOCK_FILE);
        let file = Self::io(&path, File::options().create(true).truncate(false).write(true).open(&path))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(WorkerLock { _file: file })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(source)) => Err(JobError::Io { path, source }),
        }
    }

    pub fn worker_running(&self) -> Result<bool, JobError> {
        Ok(self.try_lock_worker()?.is_none())
    }

    pub fn list(&self) -> Result<Vec<Job>, JobError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(JobError::Io { path: self.dir.clone(), source }),
        };
        let mut jobs = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            match Self::read(&path) {
                Ok(job) => jobs.push(job),
                Err(e) => tracing::warn!("skipping job file: {e}"),
            }
        }
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(jobs)
    }

    fn read(path: &Path) -> Result<Job, JobError> {
        let text = Self::io(path, fs::read_to_string(path))?;
        from_json_str(&text).map_err(|source| JobError::Invalid { path: path.to_path_buf(), source })
    }

    pub fn get(&self, id: &str) -> Result<Job, JobError> {
        let path = self.job_path(id);
        match Self::read(&path) {
            Err(JobError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => Err(JobError::NotFound(id.to_string())),
            other => other,
        }
    }

    pub fn save(&self, job: &Job) -> Result<(), JobError> {
        let path = self.job_path(&job.id);
        let text = to_json_string_pretty(job)?;
        Self::io(&path, atomic::write(&path, text.as_bytes()))
    }

    pub fn enqueue(&self, kind: JobKind, session: &str, replace: bool) -> Result<(Job, bool), JobError> {
        if let Some(existing) = self.list()?.into_iter().find(|j| j.kind == kind && j.session == session && is_active(&j.state)) {
            return Ok((existing, false));
        }
        let job = Job {
            version: JOB_VERSION,
            id: self.new_id(),
            kind,
            session: session.to_string(),
            replace,
            state: JobState::Queued,
            created_at: timestamp(),
            started_at: None,
            finished_at: None,
            stage: None,
            fraction: None,
            error: None,
        };
        self.save(&job)?;
        Ok((job, true))
    }

    pub fn cancel(&self, id: &str) -> Result<(), JobError> {
        let mut job = self.get(id)?;
        match job.state {
            JobState::Queued => {
                job.state = JobState::Cancelled;
                job.finished_at = Some(timestamp());
                self.save(&job)
            }
            JobState::Running => {
                let path = self.cancel_path(id);
                Self::io(&path, fs::write(&path, b""))
            }
            _ => Ok(()),
        }
    }

    pub fn cancel_requested(&self, id: &str) -> bool {
        self.cancel_path(id).exists()
    }

    pub fn claim_next(&self) -> Result<Option<Job>, JobError> {
        let Some(mut job) = self.list()?.into_iter().find(|j| j.state == JobState::Queued) else { return Ok(None) };
        job.state = JobState::Running;
        job.started_at = Some(timestamp());
        job.stage = None;
        job.fraction = None;
        job.error = None;
        self.save(&job)?;
        Ok(Some(job))
    }

    pub fn finish(&self, job: &mut Job, state: JobState, error: Option<String>) -> Result<(), JobError> {
        job.state = state;
        job.finished_at = Some(timestamp());
        job.error = error;
        job.stage = None;
        job.fraction = None;
        self.save(job)?;
        let _ = fs::remove_file(self.cancel_path(&job.id));
        Ok(())
    }

    pub fn requeue_stale(&self, _lock: &WorkerLock) -> Result<(), JobError> {
        for mut job in self.list()? {
            if job.state == JobState::Running {
                job.state = JobState::Queued;
                job.stage = None;
                job.fraction = None;
                self.save(&job)?;
            }
            let _ = fs::remove_file(self.cancel_path(&job.id));
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), JobError> {
        for path in [self.job_path(id), self.cancel_path(id)] {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(JobError::Io { path, source: e }),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn clear_finished(&self) -> Result<(), JobError> {
        for job in self.list()? {
            if !is_active(&job.state) {
                self.remove(&job.id)?;
            }
        }
        Ok(())
    }

    pub fn prune_finished(&self, keep: usize) -> Result<(), JobError> {
        let finished: Vec<_> = self.list()?.into_iter().filter(|j| !is_active(&j.state)).collect();
        let excess = finished.len().saturating_sub(keep);
        for job in finished.into_iter().take(excess) {
            self.remove(&job.id)?;
        }
        Ok(())
    }

    pub fn active_count(&self) -> Result<usize, JobError> {
        Ok(self.list()?.iter().filter(|j| is_active(&j.state)).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> JobStore {
        let dir = std::env::temp_dir().join(format!("audio-recorder-jobs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        JobStore::at(dir)
    }

    #[test]
    fn dedupes_per_kind() {
        let s = store("kinds");
        let (t, _) = s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        let (c, created) = s.enqueue(JobKind::Compress, "a", false).unwrap();
        assert!(created && t.id != c.id);
        assert!(!s.enqueue(JobKind::Compress, "a", false).unwrap().1);
    }

    #[test]
    fn enqueue_dedupes_active_sessions_and_orders_by_creation() {
        let s = store("enqueue");
        let (a, created) = s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        assert!(created);
        let (again, created) = s.enqueue(JobKind::Transcribe, "a", true).unwrap();
        assert!(!created);
        assert_eq!(again.id, a.id);
        let (b, _) = s.enqueue(JobKind::Transcribe, "b", false).unwrap();
        let ids: Vec<_> = s.list().unwrap().into_iter().map(|j| j.id).collect();
        assert_eq!(ids, vec![a.id.clone(), b.id]);
        assert_eq!(s.claim_next().unwrap().unwrap().id, a.id);
        assert_eq!(s.enqueue(JobKind::Transcribe, "a", false).unwrap().1, false);
    }

    #[test]
    fn cancel_queued_is_immediate_and_running_uses_marker() {
        let s = store("cancel");
        let (a, _) = s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        s.cancel(&a.id).unwrap();
        assert_eq!(s.get(&a.id).unwrap().state, JobState::Cancelled);
        let (b, _) = s.enqueue(JobKind::Transcribe, "b", false).unwrap();
        let mut running = s.claim_next().unwrap().unwrap();
        assert_eq!(running.id, b.id);
        assert!(!s.cancel_requested(&b.id));
        s.cancel(&b.id).unwrap();
        assert!(s.cancel_requested(&b.id));
        s.finish(&mut running, JobState::Cancelled, None).unwrap();
        assert!(!s.cancel_requested(&b.id));
    }

    #[test]
    fn stale_running_jobs_return_to_queue_under_the_lock() {
        let s = store("stale");
        let (a, _) = s.enqueue(JobKind::Transcribe, "a", false).unwrap();
        s.claim_next().unwrap();
        let lock = s.try_lock_worker().unwrap().unwrap();
        assert!(s.try_lock_worker().unwrap().is_none());
        assert!(s.worker_running().unwrap());
        s.requeue_stale(&lock).unwrap();
        assert_eq!(s.get(&a.id).unwrap().state, JobState::Queued);
        drop(lock);
        assert!(!s.worker_running().unwrap());
    }

    #[test]
    fn prune_keeps_newest_finished_and_all_active() {
        let s = store("prune");
        for name in ["a", "b", "c"] {
            s.enqueue(JobKind::Transcribe, name, false).unwrap();
            let mut j = s.claim_next().unwrap().unwrap();
            s.finish(&mut j, JobState::Done, None).unwrap();
        }
        let (active, _) = s.enqueue(JobKind::Transcribe, "d", false).unwrap();
        s.prune_finished(1).unwrap();
        let jobs = s.list().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].session, "c");
        assert_eq!(jobs[1].id, active.id);
        s.clear_finished().unwrap();
        assert_eq!(s.list().unwrap().len(), 1);
    }
}
