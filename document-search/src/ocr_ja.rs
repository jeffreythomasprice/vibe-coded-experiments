//! OCR for Japanese pages with NDLOCR-Lite and/or YomiToku, per
//! `[ocr].japanese_engine`. `pdf::extract` hands over the pages it picked for
//! OCR when the resolved language is Japanese; this renders them in batches,
//! runs the engines over each batch (both at once for the ensemble: NDLOCR on
//! the CPU, YomiToku on the GPU), and for the ensemble merges each page's two
//! readings via `ocr_merge`'s rules and, for what the rules leave open, a
//! vision-model arbitration asked in both reading orders.
//!
//! Engine and install failures fail the ingest: falling back to tesseract
//! would silently trade a ~1% error rate for ~16%. The one exception is an
//! arbiter reply that is still unusable after a retry, where that page falls
//! back to `Hunk::default_choice` with a warning, since everything before the
//! ingest transaction would otherwise be thrown away over one bad reply.

use std::path::{Path, PathBuf};

use tokio::process::Command;
use tokio::sync::watch;

use crate::config::{Config, JapaneseEngine, LlmProvider};
use crate::ingest::IngestError;
use crate::llm::{self, LlmError};
use crate::ocr_merge::{self, Choice};
use crate::ocr_tools::{self, Mode, Tool};
use crate::pdf::{self, ExtractProgress};
use crate::proc::{self, ProcError};

/// Lines of a failed engine's stderr quoted in the error.
const ERROR_TAIL_LINES: usize = 15;

/// Above this many disagreements the engines read the page in different
/// orders or one of them failed outright; arbitrating hundreds of spots in
/// one call isn't worth it, so the page takes the defaults.
const MAX_ARBITER_HUNKS: usize = 120;

/// Most pdftoppm processes rendering one batch at once.
const RASTER_JOBS: usize = 8;

#[derive(thiserror::Error, Debug)]
pub enum JaOcrError {
    #[error(transparent)]
    Tools(#[from] ocr_tools::ToolsError),

    #[error(
        "{tool} failed on pages {first}-{last}:\n{detail}\n(`document-search ocr-tools install \
         --reinstall` rebuilds it; [ocr].japanese_engine = \"tesseract\" avoids it)"
    )]
    Engine {
        tool: Tool,
        first: u32,
        last: u32,
        detail: String,
    },

    #[error("{tool} produced no output for any of pages {first}-{last}; see the log for its stderr")]
    NoOutput { tool: Tool, first: u32, last: u32 },

    #[error("reading {tool} output {path}: {detail}")]
    Output {
        tool: Tool,
        path: PathBuf,
        detail: String,
    },

    #[error(
        "OCR arbiter model {model:?} is not pulled in Ollama; `ollama pull {model}`, or set \
         [ocr].arbiter_model to a vision model you have, or [ocr].japanese_engine = \"ndlocr\""
    )]
    ArbiterMissing { model: String },

    #[error(
        "OCR arbiter model {model:?} can't read images (Ollama reports {caps:?}); set \
         [ocr].arbiter_model to a vision model such as qwen3.5:9b"
    )]
    ArbiterNotVision { model: String, caps: Vec<String> },

    #[error("checking OCR arbiter model {model:?}: {source}")]
    ArbiterProbe {
        model: String,
        #[source]
        source: crate::ollama::OllamaError,
    },

    #[error("OCR arbitration on page {page}: {source}")]
    Arbiter {
        page: u32,
        #[source]
        source: LlmError,
    },

    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Whether `[ocr].japanese_engine` takes over for the resolved tesseract
/// language spec `lang` (any `jpn`/`jpn_vert` component).
pub fn handles(lang: &str, engine: JapaneseEngine) -> bool {
    engine != JapaneseEngine::Tesseract
        && lang
            .split('+')
            .any(|code| matches!(code.trim(), "jpn" | "jpn_vert"))
}

fn tools_for(engine: JapaneseEngine) -> &'static [Tool] {
    match engine {
        JapaneseEngine::Ensemble => &[Tool::Ndlocr, Tool::Yomitoku],
        JapaneseEngine::Ndlocr => &[Tool::Ndlocr],
        JapaneseEngine::Yomitoku => &[Tool::Yomitoku],
        JapaneseEngine::Tesseract => &[],
    }
}

/// Per-document merge counters, logged when OCR finishes.
#[derive(Debug, Default)]
struct MergeStats {
    pages_agreed: usize,
    hunks: usize,
    /// Settled by `ocr_merge::rule_choice` without the arbiter.
    by_rule: usize,
    chose_a: usize,
    chose_b: usize,
    /// Arbiter answers that flipped with the presentation order.
    unsure: usize,
    defaulted_pages: Vec<u32>,
}

/// OCR `pages` (0-based indices into the PDF) and return their text in the
/// same order.
pub async fn run(
    pdf_path: &Path,
    pages: &[usize],
    cfg: &Config,
    http: &reqwest::Client,
    cancel: Option<&watch::Receiver<bool>>,
    on_progress: &mut (dyn FnMut(ExtractProgress) + Send),
) -> Result<Vec<String>, IngestError> {
    let engine = cfg.ocr.japanese_engine;
    if engine == JapaneseEngine::Ensemble {
        check_arbiter(cfg, http).await?;
    }
    let ensured = ocr_tools::ensure(&cfg.ocr_tools, tools_for(engine), Mode::Ensure, cancel, |s| {
        on_progress(ExtractProgress::Stage(s))
    })
    .await
    .map_err(|e| match e {
        ocr_tools::ToolsError::Cancelled => IngestError::Cancelled,
        e => JaOcrError::from(e).into(),
    })?;
    let tools = ensured.tools;

    let total = pages.len() as u32;
    let mut out: Vec<String> = Vec::with_capacity(pages.len());
    let mut stats = MergeStats::default();
    let started = std::time::Instant::now();
    for batch in pages.chunks(cfg.ocr_tools.batch_pages.max(1)) {
        // Announce the batch's first page before the engines run, so the
        // OCR total is known (and shown) during the first batch too.
        on_progress(ExtractProgress::Ocr {
            current: out.len() as u32 + 1,
            total,
        });
        let tmp = tempfile::tempdir().map_err(|source| JaOcrError::Io {
            what: "creating temp dir for OCR",
            path: std::env::temp_dir(),
            source,
        })?;
        let img_dir = tmp.path().join("img");
        mkdir(&img_dir)?;
        let page_numbers: Vec<u32> = batch.iter().map(|&i| (i + 1) as u32).collect();
        let stems: Vec<String> = page_numbers.iter().map(|n| format!("p{n:05}")).collect();
        rasterize_batch(pdf_path, &page_numbers, &stems, &img_dir, cfg.ocr.dpi, cancel).await?;
        let first = page_numbers[0];
        let last = *page_numbers.last().expect("chunks are non-empty");

        let (a, b) = tokio::try_join!(
            async {
                match &tools.ndlocr {
                    Some(t) => run_ndlocr(t, &img_dir, tmp.path(), &stems, (first, last), cancel)
                        .await
                        .map(Some),
                    None => Ok(None),
                }
            },
            async {
                match &tools.yomitoku {
                    Some(t) => run_yomitoku(t, &img_dir, tmp.path(), &stems, (first, last), cancel)
                        .await
                        .map(Some),
                    None => Ok(None),
                }
            },
        )?;

        for (k, &n) in page_numbers.iter().enumerate() {
            on_progress(ExtractProgress::Ocr {
                current: out.len() as u32 + 1,
                total,
            });
            let text = match (&a, &b) {
                (Some(a), Some(b)) => {
                    let png = img_dir.join(format!("{}.png", stems[k]));
                    merge_page(cfg, http, n, &a[k], &b[k], &png, &mut stats, cancel).await?
                }
                (Some(only), None) | (None, Some(only)) => only[k].clone(),
                (None, None) => unreachable!("tools_for never returns an empty set here"),
            };
            out.push(text);
        }
    }

    if engine == JapaneseEngine::Ensemble {
        tracing::info!(
            pages = pages.len(),
            pages_agreed = stats.pages_agreed,
            hunks = stats.hunks,
            by_rule = stats.by_rule,
            arbiter_unsure = stats.unsure,
            chose_ndlocr = stats.chose_a,
            chose_yomitoku = stats.chose_b,
            defaulted_pages = ?stats.defaulted_pages,
            secs = started.elapsed().as_secs_f64(),
            "ocr-ja: ensemble finished",
        );
    } else {
        tracing::info!(
            pages = pages.len(),
            engine = ?engine,
            secs = started.elapsed().as_secs_f64(),
            "ocr-ja: finished",
        );
    }
    Ok(out)
}

/// Render a batch's pages with up to `RASTER_JOBS` pdftoppm processes at
/// once. One at a time, a 300-dpi scan page takes about 3 s, which made
/// rendering the bulk of the OCR time. An error drops the `JoinSet`, which
/// aborts the other renders and kills their processes.
async fn rasterize_batch(
    pdf_path: &Path,
    page_numbers: &[u32],
    stems: &[String],
    img_dir: &Path,
    dpi: u32,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<(), IngestError> {
    let jobs = std::thread::available_parallelism().map_or(4, |n| n.get().min(RASTER_JOBS));
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(jobs));
    let mut set = tokio::task::JoinSet::new();
    for (&n, stem) in page_numbers.iter().zip(stems) {
        let permit = std::sync::Arc::clone(&permits)
            .acquire_owned()
            .await
            .expect("semaphore is never closed");
        let pdf = pdf_path.to_path_buf();
        let out = img_dir.join(stem);
        let cancel = cancel.cloned();
        set.spawn(async move {
            let _permit = permit;
            pdf::rasterize_page_to(&pdf, n, dpi, &out, cancel.as_ref()).await
        });
    }
    while let Some(res) = set.join_next().await {
        res.expect("rasterize task panicked")?;
    }
    Ok(())
}

fn mkdir(p: &Path) -> Result<(), JaOcrError> {
    std::fs::create_dir_all(p).map_err(|source| JaOcrError::Io {
        what: "creating",
        path: p.to_path_buf(),
        source,
    })
}

/// Fail before any OCR runs when the arbiter model can't do the job.
async fn check_arbiter(cfg: &Config, http: &reqwest::Client) -> Result<(), JaOcrError> {
    if cfg.llm.provider != LlmProvider::Ollama {
        return Ok(());
    }
    let model = &cfg.ocr.arbiter_model;
    match crate::ollama::model_capabilities(http, &cfg.ollama.url, model).await {
        Ok(None) => Err(JaOcrError::ArbiterMissing {
            model: model.clone(),
        }),
        Ok(Some(caps)) if !caps.iter().any(|c| c == "vision") => Err(JaOcrError::ArbiterNotVision {
            model: model.clone(),
            caps,
        }),
        Ok(Some(_)) => Ok(()),
        Err(source) => Err(JaOcrError::ArbiterProbe {
            model: model.clone(),
            source,
        }),
    }
}

/// Map a failed engine run to an ingest error.
fn engine_err(tool: Tool, (first, last): (u32, u32), e: ProcError) -> IngestError {
    let detail = match e {
        ProcError::Cancelled => return IngestError::Cancelled,
        ProcError::Failed(out) => proc::output_tail(&out, ERROR_TAIL_LINES),
        ProcError::NotFound => "executable missing; the install looks broken".to_string(),
        ProcError::Spawn(e) | ProcError::Wait(e) => e.to_string(),
    };
    JaOcrError::Engine {
        tool,
        first,
        last,
        detail,
    }
    .into()
}

/// Text per stem, in order. A page an engine wrote nothing for comes back
/// empty with a warning (blank pages, illustrations); an engine that wrote
/// nothing for the whole batch is broken.
fn collect_outputs(
    tool: Tool,
    stems: &[String],
    range: (u32, u32),
    mut read: impl FnMut(&str) -> Result<Option<String>, JaOcrError>,
) -> Result<Vec<String>, IngestError> {
    let mut texts = Vec::with_capacity(stems.len());
    let mut found = 0;
    for stem in stems {
        match read(stem)? {
            Some(t) => {
                found += 1;
                texts.push(t);
            }
            None => {
                tracing::warn!(tool = tool.name(), page = %stem, "ocr-ja: no output for page; treating it as blank");
                texts.push(String::new());
            }
        }
    }
    if found == 0 {
        return Err(JaOcrError::NoOutput {
            tool,
            first: range.0,
            last: range.1,
        }
        .into());
    }
    Ok(texts)
}

async fn run_ndlocr(
    t: &ocr_tools::Ndlocr,
    img_dir: &Path,
    work: &Path,
    stems: &[String],
    range: (u32, u32),
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Vec<String>, IngestError> {
    let out_dir = work.join("ndlocr");
    mkdir(&out_dir)?;
    let mut cmd = Command::new(&t.python);
    cmd.current_dir(&t.src_dir)
        .arg("ocr.py")
        .arg("--sourcedir")
        .arg(img_dir)
        .arg("--output")
        .arg(&out_dir)
        .args(["--device", "cpu"]);
    let started = std::time::Instant::now();
    proc::run(cmd, cancel)
        .await
        .map_err(|e| engine_err(Tool::Ndlocr, range, e))?;
    tracing::debug!(pages = stems.len(), secs = started.elapsed().as_secs_f64(), "ocr-ja: NDLOCR-Lite batch done");
    collect_outputs(Tool::Ndlocr, stems, range, |stem| {
        read_optional(Tool::Ndlocr, &out_dir.join(format!("{stem}.txt")))
    })
}

async fn run_yomitoku(
    t: &ocr_tools::Yomitoku,
    img_dir: &Path,
    work: &Path,
    stems: &[String],
    range: (u32, u32),
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Vec<String>, IngestError> {
    let out_dir = work.join("yomitoku");
    mkdir(&out_dir)?;
    let mut cmd = Command::new(&t.exe);
    cmd.arg(img_dir)
        .args(["-f", "json", "-o"])
        .arg(&out_dir)
        .args(["--ignore_ruby", "--ignore_meta", "-d", t.device])
        .env("HF_HOME", &t.hf_home);
    let started = std::time::Instant::now();
    proc::run(cmd, cancel)
        .await
        .map_err(|e| engine_err(Tool::Yomitoku, range, e))?;
    tracing::debug!(pages = stems.len(), secs = started.elapsed().as_secs_f64(), "ocr-ja: YomiToku batch done");

    // YomiToku names each output `<input dir>_<stem>_p1.json`; match on the
    // suffix rather than relying on the prefix.
    let names: Vec<String> = std::fs::read_dir(&out_dir)
        .map_err(|source| JaOcrError::Io {
            what: "listing",
            path: out_dir.clone(),
            source,
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    collect_outputs(Tool::Yomitoku, stems, range, |stem| {
        let suffix = format!("_{stem}_p1.json");
        let Some(name) = names.iter().find(|n| n.ends_with(&suffix)) else {
            return Ok(None);
        };
        let path = out_dir.join(name);
        let raw = read_optional(Tool::Yomitoku, &path)?.unwrap_or_default();
        yomitoku_text(&raw)
            .map(Some)
            .map_err(|detail| JaOcrError::Output {
                tool: Tool::Yomitoku,
                path,
                detail,
            })
    })
}

fn read_optional(tool: Tool, path: &Path) -> Result<Option<String>, JaOcrError> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(JaOcrError::Output {
            tool,
            path: path.to_path_buf(),
            detail: e.to_string(),
        }),
    }
}

#[derive(serde::Deserialize)]
struct YomitokuPage {
    #[serde(default)]
    paragraphs: Vec<YomitokuParagraph>,
}

#[derive(serde::Deserialize)]
struct YomitokuParagraph {
    #[serde(default)]
    contents: String,
    #[serde(default)]
    order: i64,
}

/// One line per paragraph, in YomiToku's reading order. Line breaks inside a
/// paragraph are column breaks, so they're dropped.
fn yomitoku_text(raw: &str) -> Result<String, String> {
    let mut page: YomitokuPage = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    page.paragraphs.sort_by_key(|p| p.order);
    let mut out = String::new();
    for p in page.paragraphs {
        let line: String = p.contents.chars().filter(|&c| c != '\n').collect();
        if line.trim().is_empty() {
            continue;
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
async fn merge_page(
    cfg: &Config,
    http: &reqwest::Client,
    page: u32,
    a: &str,
    b: &str,
    png: &Path,
    stats: &mut MergeStats,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, IngestError> {
    // One engine seeing nothing (YomiToku on a title page over artwork, say)
    // is a missing reading, not a disagreement to arbitrate.
    if b.trim().is_empty() || a.trim().is_empty() {
        stats.pages_agreed += 1;
        return Ok(if b.trim().is_empty() { a } else { b }.to_string());
    }
    let alignment = ocr_merge::align(a, b);
    let hunks = alignment.hunks();
    stats.by_rule += alignment.auto_resolved();
    stats.hunks += hunks.len() + alignment.auto_resolved();
    if hunks.is_empty() {
        stats.pages_agreed += 1;
        return Ok(alignment.render(&[]));
    }

    let choices = if hunks.len() > MAX_ARBITER_HUNKS {
        tracing::warn!(page, hunks = hunks.len(), "ocr-ja: engines disagree too much to arbitrate; using defaults");
        None
    } else {
        arbitrate(cfg, http, page, &hunks, png, cancel).await?
    };
    let choices = match choices {
        Some((straight, swapped)) => {
            let (choices, unsure) = ocr_merge::reconcile(&hunks, &straight, &swapped);
            stats.unsure += unsure;
            choices
        }
        None => {
            stats.defaulted_pages.push(page);
            hunks.iter().map(ocr_merge::Hunk::default_choice).collect()
        }
    };
    for c in &choices {
        match c {
            Choice::A => stats.chose_a += 1,
            Choice::B => stats.chose_b += 1,
            Choice::Text(_) => {}
        }
    }
    for (h, c) in hunks.iter().zip(&choices) {
        tracing::debug!(page, before = %h.before, a = %h.a, b = %h.b, after = %h.after, choice = ?c, "ocr-ja: hunk");
    }
    Ok(alignment.render(&choices))
}

/// Ask the arbiter twice, with each hunk's readings in both orders, and
/// return both sets of answers (in terms of the real A and B) for
/// `ocr_merge::reconcile`. `None` means a usable reply couldn't be had and the
/// caller should take the defaults.
async fn arbitrate(
    cfg: &Config,
    http: &reqwest::Client,
    page: u32,
    hunks: &[ocr_merge::Hunk],
    png: &Path,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Option<(Vec<Choice>, Vec<Choice>)>, IngestError> {
    let image = std::fs::read(png).map_err(|source| JaOcrError::Io {
        what: "reading page image",
        path: png.to_path_buf(),
        source,
    })?;
    let images = [image];
    let started = std::time::Instant::now();
    let Some(straight) = ask_arbiter(cfg, http, page, hunks, &images, false, cancel).await? else {
        return Ok(None);
    };
    let Some(swapped) = ask_arbiter(cfg, http, page, hunks, &images, true, cancel).await? else {
        return Ok(None);
    };
    tracing::debug!(page, hunks = hunks.len(), secs = started.elapsed().as_secs_f64(), "ocr-ja: arbitrated");
    Ok(Some((straight, swapped)))
}

/// One arbitration call, retried once on an unusable reply.
async fn ask_arbiter(
    cfg: &Config,
    http: &reqwest::Client,
    page: u32,
    hunks: &[ocr_merge::Hunk],
    images: &[Vec<u8>],
    swapped: bool,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Option<Vec<Choice>>, IngestError> {
    let schema = ocr_merge::answer_schema();
    let user = ocr_merge::arbiter_user_message(hunks, swapped);
    let extras = llm::Extras {
        images,
        format: Some(&schema),
        model: Some(&cfg.ocr.arbiter_model),
        temperature: Some(0.0),
    };
    // A few tokens per answer; thinking off, since the image decides.
    let overrides = llm::Overrides {
        think: Some(false),
        max_tokens: Some(256 + 32 * hunks.len() as u32),
    };
    for attempt in 1..=2 {
        // At temperature 0 a retry would repeat the bad reply word for word.
        let extras = llm::Extras {
            temperature: Some(if attempt == 1 { 0.0 } else { 0.4 }),
            ..extras
        };
        let reply = match llm::complete_ext(
            http,
            cfg,
            ocr_merge::ARBITER_SYSTEM_PROMPT,
            &user,
            overrides,
            extras,
            cancel,
        )
        .await
        {
            Ok(r) => r,
            Err(LlmError::Cancelled) => return Err(IngestError::Cancelled),
            Err(LlmError::EmptyResponse { .. }) => {
                tracing::warn!(page, attempt, "ocr-ja: arbiter returned nothing");
                continue;
            }
            Err(source) => return Err(JaOcrError::Arbiter { page, source }.into()),
        };
        match ocr_merge::parse_answers(&reply, hunks, swapped) {
            Ok(choices) => return Ok(Some(choices)),
            Err(problem) => {
                tracing::warn!(page, attempt, %problem, reply = %reply, "ocr-ja: unusable arbiter reply");
            }
        }
    }
    tracing::warn!(page, "ocr-ja: arbiter failed twice; page uses default picks (NDLOCR-Lite's reading, or text only YomiToku has)");
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_only_japanese_specs() {
        assert!(handles("jpn_vert+jpn", JapaneseEngine::Ensemble));
        assert!(handles("jpn+eng", JapaneseEngine::Ndlocr));
        assert!(!handles("jpn+eng", JapaneseEngine::Tesseract));
        assert!(!handles("chi_sim+eng", JapaneseEngine::Ensemble));
        assert!(!handles("eng", JapaneseEngine::Ensemble));
    }

    #[test]
    fn yomitoku_paragraphs_in_order_without_column_breaks() {
        let raw = r#"{"paragraphs":[
            {"contents":"二番目の\n段落。","order":1,"role":null},
            {"contents":"最初の段落。","order":0,"role":null},
            {"contents":"  ","order":2,"role":null}
        ],"tables":[],"figures":[],"words":[]}"#;
        assert_eq!(yomitoku_text(raw).unwrap(), "最初の段落。\n二番目の段落。\n");
        assert_eq!(yomitoku_text(r#"{"paragraphs":[]}"#).unwrap(), "");
        assert!(yomitoku_text("not json").is_err());
    }

    #[test]
    fn collect_outputs_blanks_missing_pages_but_not_all() {
        let stems = vec!["p00001".to_string(), "p00002".to_string()];
        let got = collect_outputs(Tool::Ndlocr, &stems, (1, 2), |s| {
            Ok((s == "p00001").then(|| "本文".to_string()))
        })
        .unwrap();
        assert_eq!(got, vec!["本文".to_string(), String::new()]);

        let err = collect_outputs(Tool::Ndlocr, &stems, (1, 2), |_| Ok(None)).unwrap_err();
        assert!(err.to_string().contains("no output for any of pages 1-2"), "{err}");
    }
}
