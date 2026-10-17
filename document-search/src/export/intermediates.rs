//! `--output-intermediates DIR`: keep every step's input and output on disk,
//! and skip a step whose files are already there.
//!
//! A step is cached when `<name>.input.txt` equals the step's current input
//! byte for byte and `<name>.txt` (its answer) or `<name>.kept` (the step
//! failed its checks and the original text was kept) exists. `lookup` writes
//! the input file on a miss and `record` writes the output last, so a cancel
//! mid-step leaves no output and the step runs again next time.
//!
//! `manifest.json` records the parameters the directory was built with;
//! reusing it with different ones is an error rather than silently mixing
//! results.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ExportError, write_atomic};

const MANIFEST: &str = "manifest.json";

/// The parameters an intermediates directory was built with. The optional
/// fields are filled in the first time a stage that depends on them runs, so
/// a `--no-edit` run doesn't constrain a later run that edits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub version: u32,
    pub document: String,
    pub document_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_chunks: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap_chunks: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_segment_bytes: Option<usize>,
}

impl Manifest {
    pub fn sha256(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    /// `self` (what is on disk) merged with `req` (what this run uses):
    /// fields only one side has are kept, fields both have must agree.
    /// Returns `(field, recorded, requested)` for the first disagreement.
    fn merge(&self, req: &Manifest) -> Result<Manifest, (&'static str, String, String)> {
        fn req_eq<T: PartialEq + ToString>(
            field: &'static str,
            a: &T,
            b: &T,
        ) -> Result<(), (&'static str, String, String)> {
            if a == b {
                Ok(())
            } else {
                Err((field, a.to_string(), b.to_string()))
            }
        }
        fn opt<T: PartialEq + ToString + Clone>(
            field: &'static str,
            a: &Option<T>,
            b: &Option<T>,
        ) -> Result<Option<T>, (&'static str, String, String)> {
            match (a, b) {
                (Some(x), Some(y)) => req_eq(field, x, y).map(|()| Some(x.clone())),
                (Some(x), None) => Ok(Some(x.clone())),
                (None, y) => Ok(y.clone()),
            }
        }
        req_eq("version", &self.version, &req.version)?;
        req_eq("document", &self.document, &req.document)?;
        req_eq("document_sha256", &self.document_sha256, &req.document_sha256)?;
        Ok(Manifest {
            version: self.version,
            document: self.document.clone(),
            document_sha256: self.document_sha256.clone(),
            llm_provider: opt("llm_provider", &self.llm_provider, &req.llm_provider)?,
            llm_model: opt("llm_model", &self.llm_model, &req.llm_model)?,
            window_chunks: opt("window_chunks", &self.window_chunks, &req.window_chunks)?,
            overlap_chunks: opt("overlap_chunks", &self.overlap_chunks, &req.overlap_chunks)?,
            edit_segment_bytes: opt(
                "edit_segment_bytes",
                &self.edit_segment_bytes,
                &req.edit_segment_bytes,
            )?,
        })
    }
}

/// What a cached step holds.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Cached {
    /// The step's answer.
    Output(String),
    /// The step failed its checks and the original text was kept.
    Kept,
}

pub(super) struct Store {
    dir: PathBuf,
}

fn io_err(path: &Path, source: std::io::Error) -> ExportError {
    ExportError::IntermediatesIo {
        path: path.to_path_buf(),
        source,
    }
}

fn read_opt(path: &Path) -> Result<Option<String>, ExportError> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, e)),
    }
}

impl Store {
    /// Create `dir` if needed and check it against `req`. The merged
    /// manifest is written back, so later runs see the fields this one added.
    pub fn open(dir: &Path, req: &Manifest) -> Result<Self, ExportError> {
        std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
        let path = dir.join(MANIFEST);
        let merged = match read_opt(&path)? {
            Some(s) => {
                let recorded: Manifest = serde_json::from_str(&s).map_err(|e| {
                    io_err(&path, std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                })?;
                recorded.merge(req).map_err(|(field, recorded, requested)| {
                    ExportError::IntermediatesMismatch {
                        dir: dir.to_path_buf(),
                        field,
                        recorded,
                        requested,
                    }
                })?
            }
            None => req.clone(),
        };
        let json = serde_json::to_string_pretty(&merged).expect("manifest serializes");
        write_atomic(&path, json.as_bytes())?;
        tracing::info!(path = %path.display(), "export: intermediates manifest");
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn step_path(&self, stage: &str, name: &str, suffix: &str) -> PathBuf {
        self.dir.join(stage).join(format!("{name}{suffix}"))
    }

    /// Whether step `stage/name` already ran on exactly `input`. On a miss it
    /// writes `input` (and clears any old output) so the step can be
    /// inspected while it runs. Logs which of the two happened, and where.
    pub fn lookup(
        &self,
        stage: &str,
        name: &str,
        what: &str,
        input: &str,
    ) -> Result<Option<Cached>, ExportError> {
        let input_path = self.step_path(stage, name, ".input.txt");
        let out_path = self.step_path(stage, name, ".txt");
        let kept_path = self.step_path(stage, name, ".kept");
        let old_input = read_opt(&input_path)?;
        let matches = old_input.as_deref() == Some(input);
        if matches {
            if let Some(out) = read_opt(&out_path)? {
                tracing::info!(path = %out_path.display(), "export: skipping {what}; reusing it");
                return Ok(Some(Cached::Output(out)));
            }
            if kept_path.exists() {
                tracing::info!(
                    path = %kept_path.display(),
                    "export: skipping {what}; it was recorded as kept unchanged",
                );
                return Ok(Some(Cached::Kept));
            }
        }
        if old_input.is_some() && !matches {
            tracing::info!(
                path = %out_path.display(),
                "export: {what}: input changed since it was cached; redoing it",
            );
        } else {
            tracing::info!(path = %out_path.display(), "export: doing {what}; it will write");
        }
        let stage_dir = self.dir.join(stage);
        std::fs::create_dir_all(&stage_dir).map_err(|e| io_err(&stage_dir, e))?;
        write_atomic(&input_path, input.as_bytes())?;
        for stale in [&out_path, &kept_path] {
            match std::fs::remove_file(stale) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_err(stale, e)),
            }
        }
        Ok(None)
    }

    /// Record the answer of a step `lookup` just missed on; `None` marks it
    /// as kept unchanged.
    pub fn record(&self, stage: &str, name: &str, output: Option<&str>) -> Result<(), ExportError> {
        match output {
            Some(text) => write_atomic(&self.step_path(stage, name, ".txt"), text.as_bytes()),
            None => write_atomic(&self.step_path(stage, name, ".kept"), b""),
        }
    }

    /// Write a whole-document stage file (`source.txt`, `translated-en.txt`, …).
    pub fn write_whole(&self, file: &str, text: &str) -> Result<(), ExportError> {
        let path = self.dir.join(file);
        tracing::info!(path = %path.display(), "export: writing {file}");
        write_atomic(&path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            version: 1,
            document: "/d.pdf".into(),
            document_sha256: Manifest::sha256("text"),
            llm_provider: Some("ollama".into()),
            llm_model: Some("m".into()),
            window_chunks: Some(4),
            overlap_chunks: Some(1),
            edit_segment_bytes: None,
        }
    }

    fn mismatch(r: Result<Store, ExportError>) -> &'static str {
        match r {
            Err(ExportError::IntermediatesMismatch { field, .. }) => field,
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("expected a mismatch"),
        }
    }

    #[test]
    fn manifest_is_written_and_reusable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("new/sub");
        Store::open(&dir, &manifest()).unwrap();
        assert!(dir.join(MANIFEST).exists());
        Store::open(&dir, &manifest()).unwrap();
    }

    #[test]
    fn changed_parameters_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        Store::open(tmp.path(), &manifest()).unwrap();
        let with = |f: fn(&mut Manifest)| {
            let mut m = manifest();
            f(&mut m);
            Store::open(tmp.path(), &m)
        };
        assert_eq!(mismatch(with(|m| m.window_chunks = Some(6))), "window_chunks");
        assert_eq!(mismatch(with(|m| m.overlap_chunks = Some(0))), "overlap_chunks");
        assert_eq!(mismatch(with(|m| m.llm_model = Some("x".into()))), "llm_model");
        assert_eq!(
            mismatch(with(|m| m.document_sha256 = Manifest::sha256("other"))),
            "document_sha256"
        );
    }

    #[test]
    fn fields_a_stage_adds_later_are_filled_in_not_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        // First run: no edit stage, no LLM fields it didn't use.
        let mut first = manifest();
        first.llm_provider = None;
        first.llm_model = None;
        Store::open(tmp.path(), &first).unwrap();
        // A run that edits adds its fields; a later one must then agree.
        let mut second = manifest();
        second.edit_segment_bytes = Some(4000);
        Store::open(tmp.path(), &second).unwrap();
        let mut third = second.clone();
        third.edit_segment_bytes = Some(2000);
        assert_eq!(mismatch(Store::open(tmp.path(), &third)), "edit_segment_bytes");
        // A run that doesn't use the field is unconstrained by it.
        let mut fourth = second.clone();
        fourth.edit_segment_bytes = None;
        Store::open(tmp.path(), &fourth).unwrap();
    }

    #[test]
    fn lookup_misses_then_hits_and_notices_changed_input() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::open(tmp.path(), &manifest()).unwrap();
        assert_eq!(s.lookup("st", "a", "step a", "in").unwrap(), None);
        // Input written, no output yet (e.g. cancelled mid-step): still a miss.
        assert!(tmp.path().join("st/a.input.txt").exists());
        assert_eq!(s.lookup("st", "a", "step a", "in").unwrap(), None);
        s.record("st", "a", Some("out")).unwrap();
        assert_eq!(
            s.lookup("st", "a", "step a", "in").unwrap(),
            Some(Cached::Output("out".into()))
        );
        // Different input: miss, and the old output is cleared.
        assert_eq!(s.lookup("st", "a", "step a", "in2").unwrap(), None);
        assert!(!tmp.path().join("st/a.txt").exists());
    }

    #[test]
    fn kept_marker_round_trips_and_is_replaced_by_output() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::open(tmp.path(), &manifest()).unwrap();
        assert_eq!(s.lookup("st", "k", "step k", "in").unwrap(), None);
        s.record("st", "k", None).unwrap();
        assert_eq!(s.lookup("st", "k", "step k", "in").unwrap(), Some(Cached::Kept));
        assert_eq!(s.lookup("st", "k", "step k", "changed").unwrap(), None);
        s.record("st", "k", Some("now")).unwrap();
        assert_eq!(
            s.lookup("st", "k", "step k", "changed").unwrap(),
            Some(Cached::Output("now".into()))
        );
    }
}
