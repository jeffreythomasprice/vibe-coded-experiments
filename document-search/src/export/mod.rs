//! `export`: write a whole ingested document as txt, markdown or PDF,
//! optionally translated by the LLM.
//!
//! The document's text isn't stored whole, so it is rebuilt from the
//! overlapping `document_chunk` rows (the same offset-based stitching as
//! `text`). Without `--to` this only reads, through a fresh connection, and
//! the connection task runs it inline. With `--to` it is a cancellable
//! worker job: chunks are translated in overlapping windows and each seam
//! is merged by a second LLM pass (see `translate`).
//!
//! Unless disabled, the final text then goes through a grammar-edit pass
//! (see `edit`) that repairs OCR and chunking errors without rewording. That
//! needs the LLM too, so only `--no-edit` without `--to` runs inline.
//!
//! Structure (headings, list items) is guessed from the text alone by
//! `blocks`; markdown and PDF render the same blocks, so they agree.
//!
//! With `--output-intermediates DIR` every LLM step (each translated window,
//! seam merge and edited segment) also leaves its input and answer in `DIR`,
//! and a step whose files are already there is skipped (see `intermediates`).
//! Without it everything stays in memory. The final outputs are still only
//! written once the whole export has succeeded.

mod blocks;
mod edit;
mod intermediates;
mod pdf;
mod render;
mod translate;

use std::path::{Path, PathBuf};

use tokio::sync::{mpsc, watch};

use crate::commands::{self, CommandError};
use crate::config::{self, Config, ExportConfig};
use crate::db::Db;
use crate::language;
use crate::llm::LlmError;
use intermediates::{Manifest, Store};

use crate::protocol::{Event, ExportFormat, ExportOutput, OutputMode, ProgressEnvelope, ProgressEvent, Request};

#[derive(thiserror::Error, Debug)]
pub enum ExportError {
    #[error(transparent)]
    Command(#[from] CommandError),

    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error("reading chunks ({op}): {source}")]
    Db {
        op: &'static str,
        #[source]
        source: turso::Error,
    },

    #[error("opening a read connection: {0}")]
    Connect(#[from] crate::db::DbError),

    #[error("document {path} has no text")]
    NoText { path: PathBuf },

    #[error(
        "stored chunks do not cover byte {missing}; this should be impossible — the DB may be corrupted"
    )]
    ChunkGap { missing: u64 },

    #[error("{what}: {source}")]
    Llm {
        what: String,
        #[source]
        source: LlmError,
    },

    #[error(
        "translation of window {window} did not come back in {expected} after a retry; \
         try a smaller --window-chunks"
    )]
    WrongLanguage { window: usize, expected: String },

    #[error("export cancelled")]
    Cancelled,

    #[error("PDF output needs a file: pass --output-pdf PATH")]
    PdfNeedsOutput,

    #[error("only one output can go to stdout")]
    TooManyStdout,

    #[error("no output requested")]
    NoOutputs,

    #[error("{path} is requested as more than one output")]
    DuplicateOutput { path: PathBuf },

    #[error("translation needs the LLM, which this export was not given (internal error)")]
    NoLlm,

    #[error(
        "intermediates directory {dir} was built with {field} = {recorded}, but this export \
         uses {requested}; use another --output-intermediates directory or delete this one"
    )]
    IntermediatesMismatch {
        dir: PathBuf,
        field: &'static str,
        recorded: String,
        requested: String,
    },

    #[error("intermediates {path}: {source}")]
    IntermediatesIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("reading font {path}: {source}")]
    FontRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("font {name}: {message}")]
    Font { name: String, message: String },

    #[error("rendering PDF: {0}")]
    Pdf(String),

    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What `run` needs to translate: the LLM settings and an HTTP client.
pub struct Llm<'a> {
    pub cfg: &'a Config,
    pub http: &'a reqwest::Client,
}

/// A validated export request.
#[derive(Debug, Clone)]
pub struct Params {
    pub path: PathBuf,
    pub outputs: Vec<ExportOutput>,
    /// Normalized target language; `None` keeps the text as written.
    pub to_language: Option<String>,
    pub window_chunks: usize,
    pub overlap_chunks: usize,
    /// Run the grammar-edit pass.
    pub edit: bool,
    pub edit_segment_bytes: usize,
    /// Directory that keeps (and reuses) every intermediate step.
    pub intermediates: Option<PathBuf>,
}

impl Params {
    /// Apply the config defaults and CLI overrides, and check the result.
    pub fn new(
        path: PathBuf,
        outputs: Vec<ExportOutput>,
        to_language: Option<&str>,
        window_chunks: Option<usize>,
        overlap_chunks: Option<usize>,
        edit: Option<bool>,
        cfg: &ExportConfig,
    ) -> Result<Self, ExportError> {
        if outputs.is_empty() {
            return Err(ExportError::NoOutputs);
        }
        if outputs
            .iter()
            .any(|o| o.format == ExportFormat::Pdf && o.path.is_none())
        {
            return Err(ExportError::PdfNeedsOutput);
        }
        if outputs.iter().filter(|o| o.path.is_none()).count() > 1 {
            return Err(ExportError::TooManyStdout);
        }
        for (i, o) in outputs.iter().enumerate() {
            if let Some(p) = &o.path
                && outputs[..i].iter().any(|prev| prev.path.as_ref() == Some(p))
            {
                return Err(ExportError::DuplicateOutput { path: p.clone() });
            }
        }
        let to_language = to_language
            .map(language::normalize)
            .filter(|l| !l.is_empty());
        let window_chunks = window_chunks.unwrap_or(cfg.window_chunks);
        let overlap_chunks = overlap_chunks.unwrap_or(cfg.overlap_chunks);
        if to_language.is_some() {
            config::validate_export_windows(window_chunks, overlap_chunks)?;
        }
        Ok(Self {
            path,
            outputs,
            to_language,
            window_chunks,
            overlap_chunks,
            edit: edit.unwrap_or(cfg.edit),
            edit_segment_bytes: cfg.edit_segment_bytes,
            intermediates: None,
        })
    }

    pub fn with_intermediates(mut self, dir: Option<PathBuf>) -> Self {
        self.intermediates = dir;
        self
    }
}

/// What an export produced.
#[derive(Debug)]
pub struct Outcome {
    pub path: PathBuf,
    pub source_language: Option<String>,
    /// Language of the output text: the target when translated, else the
    /// document's own.
    pub language: Option<String>,
    pub translated: bool,
    /// `--to` named the document's own language, so nothing was translated.
    pub translation_skipped: bool,
    /// The grammar-edit pass ran.
    pub edited: bool,
    /// Segments the edit pass left as they were because its answer failed
    /// the checks.
    pub edit_segments_kept: usize,
    /// The intermediates directory used, if any.
    pub intermediates: Option<PathBuf>,
    pub outputs: Vec<WrittenOutput>,
    /// txt / markdown returned to the client instead of a file.
    pub text: Option<String>,
}

/// One produced output; `path: None` is the one returned as `Outcome::text`.
#[derive(Debug)]
pub struct WrittenOutput {
    pub format: ExportFormat,
    pub path: Option<PathBuf>,
    pub bytes: usize,
}

impl Outcome {
    /// Text mode: the document itself when one output went to stdout (so it
    /// pipes cleanly), else a one-line note per file.
    pub fn render_text(&self) -> String {
        if let Some(text) = &self.text {
            return text.clone();
        }
        let mut suffix = String::new();
        if self.translated {
            suffix.push_str(&format!(
                ", translated {}→{}",
                self.source_language.as_deref().unwrap_or("?"),
                self.language.as_deref().unwrap_or("?"),
            ));
        } else if self.translation_skipped {
            suffix.push_str(&format!(
                ", already in {}; not translated",
                self.language.as_deref().unwrap_or("?"),
            ));
        }
        if self.edited {
            suffix.push_str(", edited");
            if self.edit_segments_kept > 0 {
                suffix.push_str(&format!(
                    " ({} segments left unedited)",
                    self.edit_segments_kept
                ));
            }
        }
        self.outputs
            .iter()
            .map(|o| {
                let out = o.path.as_deref().unwrap_or(Path::new("?"));
                format!(
                    "wrote {} ({}, {} bytes{suffix})\n",
                    out.display(),
                    o.format.as_str(),
                    o.bytes
                )
            })
            .collect()
    }

    pub fn render_json(&self) -> String {
        let outputs: Vec<_> = self
            .outputs
            .iter()
            .map(|o| {
                serde_json::json!({
                    "format": o.format.as_str(),
                    "path": o.path.as_ref().map(|p| p.display().to_string()),
                    "bytes": o.bytes,
                })
            })
            .collect();
        serde_json::json!({
            "ok": true,
            "path": self.path.display().to_string(),
            "source_language": self.source_language,
            "language": self.language,
            "translated": self.translated,
            "translation_skipped": self.translation_skipped,
            "edited": self.edited,
            "edit_segments_kept": self.edit_segments_kept,
            "intermediates": self.intermediates.as_ref().map(|p| p.display().to_string()),
            "outputs": outputs,
            "text": self.text,
        })
        .to_string()
    }
}

pub(crate) struct Chunk {
    pub byte_start: u64,
    pub byte_end: u64,
    pub text: String,
}

/// Every chunk of a document, in order.
async fn load_chunks(
    conn: &turso::Connection,
    document_id: i64,
) -> Result<Vec<Chunk>, ExportError> {
    let db = |op| move |source| ExportError::Db { op, source };
    let mut rows = conn
        .query(
            "SELECT byte_start, byte_end, text FROM document_chunk \
             WHERE document_id = ? ORDER BY chunk_index",
            (document_id,),
        )
        .await
        .map_err(db("load_chunks"))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db("load_chunks.next"))? {
        let byte_start: i64 = row.get(0).map_err(db("load_chunks.byte_start"))?;
        let byte_end: i64 = row.get(1).map_err(db("load_chunks.byte_end"))?;
        let text: String = row.get(2).map_err(db("load_chunks.text"))?;
        out.push(Chunk {
            byte_start: byte_start as u64,
            byte_end: byte_end as u64,
            text,
        });
    }
    Ok(out)
}

/// The text covered by `chunks[range]`, with the overlap between neighbours
/// counted once. Chunk offsets are into the canonical document text, so the
/// part of a chunk before the running cursor is exactly the previous chunk's
/// tail.
pub(crate) fn stitch(
    chunks: &[Chunk],
    range: std::ops::Range<usize>,
) -> Result<String, ExportError> {
    let slice = &chunks[range];
    let Some(first) = slice.first() else {
        return Ok(String::new());
    };
    let mut cursor = first.byte_start;
    let mut out = String::new();
    for c in slice {
        if c.byte_end <= cursor {
            continue;
        }
        if c.byte_start > cursor {
            return Err(ExportError::ChunkGap { missing: cursor });
        }
        let from = (cursor - c.byte_start) as usize;
        let part = c
            .text
            .get(from..)
            .ok_or(ExportError::ChunkGap { missing: cursor })?;
        out.push_str(part);
        cursor = c.byte_end;
    }
    Ok(out)
}

/// Serve a `Request::Export` end to end: validate, run on a fresh
/// connection (never the shared one an ingest may have a transaction open
/// on), and format the result for `mode`.
pub async fn handle(
    db: &Db,
    export_cfg: &ExportConfig,
    llm: Option<Llm<'_>>,
    req: &Request,
    mode: OutputMode,
    progress: Option<&mpsc::UnboundedSender<Event>>,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, ExportError> {
    let Request::Export {
        path,
        outputs,
        to_language,
        window_chunks,
        overlap_chunks,
        edit,
        intermediates,
    } = req
    else {
        unreachable!("export::handle is only called for Request::Export");
    };
    let params = Params::new(
        path.clone(),
        outputs.clone(),
        to_language.as_deref(),
        *window_chunks,
        *overlap_chunks,
        *edit,
        export_cfg,
    )?
    .with_intermediates(intermediates.clone());
    let conn = db.fresh_conn().await?;
    let outcome = run(&conn, export_cfg, llm, &params, progress, cancel).await?;
    Ok(match mode {
        OutputMode::Text => outcome.render_text(),
        OutputMode::Json => outcome.render_json(),
    })
}

fn emit_stage(progress: Option<&mpsc::UnboundedSender<Event>>, name: &str) {
    if let Some(tx) = progress {
        let _ = tx.send(Event::Progress(ProgressEnvelope {
            event: ProgressEvent::Stage {
                name: name.to_string(),
            },
            overall: None,
        }));
    }
}

/// Run an export against `conn`, which must not be a connection with an
/// ingest transaction open on it (use `Db::fresh_conn`). `llm` is required
/// when `p.to_language` is set.
pub async fn run(
    conn: &turso::Connection,
    export_cfg: &ExportConfig,
    llm: Option<Llm<'_>>,
    p: &Params,
    progress: Option<&mpsc::UnboundedSender<Event>>,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Outcome, ExportError> {
    let path = commands::normalize_path(&p.path);
    let doc = commands::lookup_document(conn, &path).await?;
    let chunks = load_chunks(conn, doc.id).await?;
    let full = stitch(&chunks, 0..chunks.len())?;
    if full.trim().is_empty() {
        return Err(ExportError::NoText { path });
    }

    let target = p.to_language.as_deref();
    let skipped = target.is_some() && target == doc.language.as_deref();
    let translated = target.is_some() && !skipped;
    // Language of the text being produced; names the per-language files.
    let out_lang = if translated { target } else { doc.language.as_deref() };
    let lang_tag = out_lang.unwrap_or("source");
    let store = match &p.intermediates {
        Some(dir) => {
            let uses_llm = translated || p.edit;
            let manifest = Manifest {
                version: 1,
                document: path.display().to_string(),
                document_sha256: Manifest::sha256(&full),
                llm_provider: match (uses_llm, llm.as_ref()) {
                    (true, Some(l)) => Some(format!("{:?}", l.cfg.llm.provider).to_lowercase()),
                    _ => None,
                },
                llm_model: match (uses_llm, llm.as_ref()) {
                    (true, Some(l)) => Some(match l.cfg.llm.provider {
                        config::LlmProvider::Ollama => l.cfg.ollama.chat_model.clone(),
                        config::LlmProvider::Anthropic => l.cfg.anthropic.model.clone(),
                    }),
                    _ => None,
                },
                window_chunks: translated.then_some(p.window_chunks),
                overlap_chunks: translated.then_some(p.overlap_chunks),
                edit_segment_bytes: p.edit.then_some(p.edit_segment_bytes),
            };
            let store = Store::open(dir, &manifest)?;
            store.write_whole("source.txt", &full)?;
            Some(store)
        }
        None => None,
    };
    let text = match target {
        Some(target) if translated => {
            let llm = llm.as_ref().ok_or(ExportError::NoLlm)?;
            emit_stage(progress, "translating");
            translate::translate_document(
                llm,
                &chunks,
                target,
                p.window_chunks,
                p.overlap_chunks,
                store.as_ref(),
                progress,
                cancel,
            )
            .await?
        }
        _ => full,
    };
    drop(chunks);
    if let (Some(store), true) = (&store, translated) {
        store.write_whole(&format!("translated-{lang_tag}.txt"), &text)?;
    }
    let (text, edit_segments_kept) = if p.edit {
        let llm = llm.as_ref().ok_or(ExportError::NoLlm)?;
        emit_stage(progress, "editing");
        let edited = edit::edit_document(
            llm,
            &text,
            out_lang,
            p.edit_segment_bytes,
            store.as_ref(),
            progress,
            cancel,
        )
        .await?;
        if let Some(store) = &store {
            store.write_whole(&format!("edited-{lang_tag}.txt"), &edited.text)?;
        }
        (edited.text, edited.kept)
    } else {
        (text, 0)
    };
    if let Some(rx) = cancel
        && *rx.borrow()
    {
        return Err(ExportError::Cancelled);
    }

    emit_stage(progress, "rendering");
    // Render every format before writing any file, so a failure (e.g. no
    // font for the PDF) leaves nothing behind.
    let parsed = if p.outputs.iter().any(|o| o.format != ExportFormat::Txt) {
        blocks::parse_blocks(&text)
    } else {
        Vec::new()
    };
    let mut rendered: Vec<(&ExportOutput, Vec<u8>)> = Vec::new();
    for o in &p.outputs {
        let bytes = match o.format {
            ExportFormat::Txt => render::to_txt(&text).into_bytes(),
            ExportFormat::Md => render::to_markdown(&parsed).into_bytes(),
            ExportFormat::Pdf => {
                let parsed = parsed.clone();
                let wants_cjk = text.chars().any(blocks::is_cjk);
                let (font_path, font_index) =
                    (export_cfg.pdf_font.clone(), export_cfg.pdf_font_index);
                tokio::task::spawn_blocking(move || {
                    let font = pdf::load_font(&font_path, font_index, wants_cjk)?;
                    pdf::render(&parsed, &font).map(|(bytes, _)| bytes)
                })
                .await
                .map_err(|e| ExportError::Pdf(format!("render task failed: {e}")))??
            }
        };
        rendered.push((o, bytes));
    }

    let mut written = Vec::new();
    let mut text_out = None;
    for (o, bytes) in rendered {
        let len = bytes.len();
        match &o.path {
            Some(path) => write_atomic(path, &bytes)?,
            None => {
                text_out =
                    Some(String::from_utf8(bytes).map_err(|_| ExportError::PdfNeedsOutput)?)
            }
        }
        written.push(WrittenOutput {
            format: o.format,
            path: o.path.clone(),
            bytes: len,
        });
    }
    Ok(Outcome {
        path: p.path.clone(),
        source_language: doc.language.clone(),
        language: if translated {
            target.map(str::to_string)
        } else {
            doc.language
        },
        translated,
        translation_skipped: skipped,
        edited: p.edit,
        edit_segments_kept,
        intermediates: store.as_ref().map(|s| s.dir().to_path_buf()),
        outputs: written,
        text: text_out,
    })
}

/// Write via a temp file in the destination directory and rename, so a
/// failure never leaves a half-written output (or clobbers an existing one).
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    use std::io::Write;
    let werr = |source| ExportError::Write {
        path: path.to_path_buf(),
        source,
    };
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(werr)?;
    tmp.write_all(bytes).map_err(werr)?;
    // The temp file is private (0600); the output should look like any file
    // the user creates, or keep the mode of the one it replaces.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).map_or(0o644, |m| m.permissions().mode() & 0o7777);
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(werr)?;
    }
    tmp.persist(path).map_err(|e| werr(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(start: u64, end: u64, text: &str) -> Chunk {
        Chunk {
            byte_start: start,
            byte_end: end,
            text: text.to_string(),
        }
    }

    /// "0123456789abcdef" cut into overlapping chunks.
    fn chunks() -> Vec<Chunk> {
        vec![
            chunk(0, 6, "012345"),
            chunk(4, 10, "456789"),
            chunk(8, 16, "89abcdef"),
        ]
    }

    #[test]
    fn stitch_counts_overlap_once() {
        let c = chunks();
        assert_eq!(stitch(&c, 0..3).unwrap(), "0123456789abcdef");
        assert_eq!(stitch(&c, 0..2).unwrap(), "0123456789");
        assert_eq!(stitch(&c, 1..3).unwrap(), "456789abcdef");
        assert_eq!(stitch(&c, 2..3).unwrap(), "89abcdef");
        assert_eq!(stitch(&c, 1..1).unwrap(), "");
    }

    #[test]
    fn stitch_reports_a_gap() {
        let c = vec![chunk(0, 4, "0123"), chunk(6, 10, "6789")];
        assert!(matches!(
            stitch(&c, 0..2),
            Err(ExportError::ChunkGap { missing: 4 })
        ));
    }

    #[test]
    fn params_validate_windows_only_when_translating() {
        let cfg = ExportConfig {
            window_chunks: 4,
            overlap_chunks: 1,
            pdf_font: String::new(),
            pdf_font_index: 0,
            edit: true,
            edit_segment_bytes: 4000,
        };
        let out = |fmt, p: Option<&str>| ExportOutput {
            format: fmt,
            path: p.map(PathBuf::from),
        };
        let new = |outs: Vec<ExportOutput>, to: Option<&str>, w, o| {
            Params::new(PathBuf::from("/d.pdf"), outs, to, w, o, None, &cfg)
        };
        let txt = || vec![out(ExportFormat::Txt, None)];
        assert!(new(txt(), None, Some(1), Some(5)).is_ok());
        assert!(new(txt(), Some("en"), Some(2), Some(2)).is_err());
        assert!(new(txt(), Some("en"), Some(0), Some(0)).is_err());
        let ok = new(vec![out(ExportFormat::Md, None)], Some(" EN "), None, None).unwrap();
        assert_eq!(ok.to_language.as_deref(), Some("en"));
        assert_eq!((ok.window_chunks, ok.overlap_chunks), (4, 1));
    }

    #[test]
    fn params_edit_follows_config_unless_overridden() {
        let cfg = ExportConfig {
            window_chunks: 4,
            overlap_chunks: 1,
            pdf_font: String::new(),
            pdf_font_index: 0,
            edit: true,
            edit_segment_bytes: 4000,
        };
        let outs = || vec![ExportOutput { format: ExportFormat::Txt, path: None }];
        let new = |edit| {
            Params::new(PathBuf::from("/d.pdf"), outs(), None, None, None, edit, &cfg).unwrap()
        };
        assert!(new(None).edit);
        assert!(new(Some(true)).edit);
        assert!(!new(Some(false)).edit);
    }

    #[test]
    fn params_validate_outputs() {
        let cfg = ExportConfig {
            window_chunks: 4,
            overlap_chunks: 1,
            pdf_font: String::new(),
            pdf_font_index: 0,
            edit: true,
            edit_segment_bytes: 4000,
        };
        let out = |fmt, p: Option<&str>| ExportOutput {
            format: fmt,
            path: p.map(PathBuf::from),
        };
        let new = |outs| Params::new(PathBuf::from("/d.pdf"), outs, None, None, None, None, &cfg);
        assert!(matches!(new(vec![]), Err(ExportError::NoOutputs)));
        assert!(matches!(
            new(vec![out(ExportFormat::Pdf, None)]),
            Err(ExportError::PdfNeedsOutput)
        ));
        assert!(matches!(
            new(vec![out(ExportFormat::Txt, None), out(ExportFormat::Md, None)]),
            Err(ExportError::TooManyStdout)
        ));
        assert!(matches!(
            new(vec![
                out(ExportFormat::Txt, Some("/a")),
                out(ExportFormat::Md, Some("/a"))
            ]),
            Err(ExportError::DuplicateOutput { .. })
        ));
        assert!(
            new(vec![
                out(ExportFormat::Txt, None),
                out(ExportFormat::Md, Some("/a.md")),
                out(ExportFormat::Pdf, Some("/a.pdf"))
            ])
            .is_ok()
        );
    }

    /// With every step already on disk, no LLM call is made: the config
    /// points at a port nothing listens on, so a call would fail the export.
    #[tokio::test]
    async fn cached_steps_make_no_llm_calls() {
        let mut cfg = config::embedded_defaults();
        cfg.ollama.url = "http://127.0.0.1:1".to_string();
        let http = reqwest::Client::new();
        let llm = Llm { cfg: &cfg, http: &http };
        let tmp = tempfile::tempdir().unwrap();
        let manifest = Manifest {
            version: 1,
            document: "/d.pdf".into(),
            document_sha256: Manifest::sha256("x"),
            ..Default::default()
        };
        let store = Store::open(tmp.path(), &manifest).unwrap();

        // Three one-chunk windows, no overlap (so no seams).
        let c = chunks();
        for (i, src) in ["012345", "456789", "89abcdef"].into_iter().enumerate() {
            let name = format!("window-{:04}_chunks-{i:04}-{i:04}", i + 1);
            assert!(store.lookup("translate-en", &name, "w", src).unwrap().is_none());
            store.record("translate-en", &name, Some(&format!("T{i}."))).unwrap();
        }
        let got = translate::translate_document(
            &llm, &c, "en", 1, 0, Some(&store), None, None,
        )
        .await
        .unwrap();
        assert_eq!(got, "T0. T1. T2.");

        let text = "Hello world.\n\nSecond paragraph.";
        assert!(store.lookup("edit-en", "segment-0001", "s", text).unwrap().is_none());
        store.record("edit-en", "segment-0001", Some("Edited.")).unwrap();
        let edited = edit::edit_document(&llm, text, Some("en"), 4000, Some(&store), None, None)
            .await
            .unwrap();
        assert_eq!((edited.text.as_str(), edited.kept), ("Edited.", 0));

        // A kept marker counts as a kept segment and leaves the original.
        assert!(store.lookup("edit-en", "segment-0001", "s", "Other text.").unwrap().is_none());
        store.record("edit-en", "segment-0001", None).unwrap();
        let kept = edit::edit_document(&llm, "Other text.", Some("en"), 4000, Some(&store), None, None)
            .await
            .unwrap();
        assert_eq!((kept.text.as_str(), kept.kept), ("Other text.", 1));
    }

    #[test]
    fn atomic_write_replaces_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(write_atomic(&dir.path().join("missing/out.txt"), b"x").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o644);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            write_atomic(&path, b"three").unwrap();
            assert_eq!(mode(&path), 0o600);
        }
    }
}
