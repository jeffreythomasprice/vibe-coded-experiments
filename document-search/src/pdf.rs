//! PDF text extraction. Shells out to `pdftotext` (poppler-utils) to produce
//! a single canonical text string with per-page byte/char offsets. pdftotext
//! emits each page followed by a U+000C form-feed; we split on it, then re-
//! join with the same separator so pages are separated *by*, not terminated
//! *with*, the form-feed. Chunk indices and `total_size_bytes` are computed
//! against this canonical string.
//!
//! When pdftotext returns garbage for a page (typical for PDFs that embed CID
//! fonts without a ToUnicode CMap — the glyph IDs have no Unicode mapping),
//! `decodability_score` flags the page and we fall back to OCR via `pdftoppm`
//! + `tesseract` for that page only. Pages pdftotext got right keep their
//! pdftotext output. The byte/char offset invariant still holds because
//! offsets are computed from the final per-page strings *after* any OCR
//! substitutions.
//!
//! Pages are picked for OCR by `select_ocr_pages`: garbled pages (above),
//! sparse pages when enough of the document is sparse (the signature of an
//! image-only scan, which pdftotext returns as *empty* rather than garbled),
//! every page when the whole text layer is empty, or every page with
//! `--force-ocr`. The decodability allowlist and the OCR language follow the
//! configured tesseract language spec; `"auto"` resolves it from tesseract's
//! script detection (OSD) on the first page that needs OCR.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use tokio::process::Command;
use tokio::sync::watch;

use crate::config::OcrConfig;
use crate::ingest::IngestError;

const PAGE_SEPARATOR: &str = "\u{000C}";

#[derive(Debug, Clone)]
pub struct PageOffset {
    pub page_number: u32,
    pub byte_start: usize,
    pub byte_end: usize,
    pub char_start: usize,
    pub char_end: usize,
}

#[derive(Debug, Clone)]
pub struct PdfText {
    pub full_text: String,
    pub page_offsets: Vec<PageOffset>,
}

/// Progress events surfaced by `extract`. The caller maps these onto whatever
/// transport-level progress events it wants. `Pdftotext.total` is the
/// document's total page count. `Ocr.total` is the count of bad pages OCR is
/// running on, *not* the document's total page count.
#[derive(Debug, Clone, Copy)]
pub enum ExtractProgress {
    Pdftotext { current: u32, total: u32 },
    Ocr { current: u32, total: u32 },
}

/// Per-ingest overrides for extraction (CLI `--force-ocr`, `--ocr-language`).
#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    /// OCR every page, ignoring the pdftotext text layer. Applies even when
    /// `[ocr].enabled = false`.
    pub force_ocr: bool,
    /// Replaces `[ocr].language` (including `"auto"`).
    pub language_override: Option<String>,
}

/// `[ocr].language` value that asks for script detection via tesseract OSD.
const AUTO_LANGUAGE: &str = "auto";

/// How many candidate pages to try OSD on before giving up. Blank or
/// image-only-but-textless pages make OSD fail, so try a few.
const OSD_MAX_ATTEMPTS: usize = 5;

/// Extract canonical PDF text. First runs pdftotext over the whole document,
/// then picks pages to re-extract via `pdftoppm` + `tesseract` (one page at a
/// time; see `select_ocr_pages`). After OCR, pages still below
/// `ocr.min_decodable_ratio` are fatal once they exceed
/// `ocr.max_undecodable_page_ratio` of the document (below that they're kept
/// with a warning), as is a document with no text at
/// all: we return `IngestError::PdfExtract` so the caller can refuse the
/// ingest rather than indexing garbage or nothing.
pub async fn extract<F>(
    path: &Path,
    ocr: &OcrConfig,
    opts: &ExtractOptions,
    cancel: Option<&watch::Receiver<bool>>,
    mut on_progress: F,
) -> Result<PdfText, IngestError>
where
    F: FnMut(ExtractProgress),
{
    let started = Instant::now();
    tracing::info!(path = %path.display(), "pdf: extracting via pdftotext");

    let stdout = run_cmd(
        "pdftotext",
        &[
            OsStr::new("-q"),
            OsStr::new("-enc"),
            OsStr::new("UTF-8"),
            path.as_os_str(),
            OsStr::new("-"),
        ],
        "poppler-utils",
        cancel,
    )
    .await?;

    // pdftotext emits each page followed by a form-feed. Strip the trailing
    // one so a 3-page doc becomes "p1<FF>p2<FF>p3" rather than "...p3<FF>",
    // matching the canonical-text invariant the rest of the codebase relies
    // on (offsets line up with the same string we hand to chunking).
    let raw = String::from_utf8_lossy(&stdout).into_owned();
    let mut pages: Vec<String> = if raw.is_empty() {
        Vec::new()
    } else {
        raw.strip_suffix(PAGE_SEPARATOR)
            .unwrap_or(&raw)
            .split(PAGE_SEPARATOR)
            .map(|s| s.to_string())
            .collect()
    };

    let total_pages = pages.len() as u32;
    for (i, _) in pages.iter().enumerate() {
        on_progress(ExtractProgress::Pdftotext {
            current: (i + 1) as u32,
            total: total_pages,
        });
    }

    let lang_spec = opts
        .language_override
        .clone()
        .unwrap_or_else(|| ocr.language.clone());
    let initial_scripts = ScriptSet::from_tesseract_langs(&lang_spec);
    let selection = select_ocr_pages(&pages, ocr, initial_scripts, opts.force_ocr);
    let bad = &selection.pages;

    if !bad.is_empty() {
        if !ocr.enabled && !opts.force_ocr {
            let (prefix, hint) = if selection.garbled > 0 {
                (
                    "pdftotext produced unreadable output",
                    "Likely a CID font without ToUnicode CMap. Set [ocr].enabled = true in config.toml to fall back to OCR, or pass --force-ocr.",
                )
            } else {
                (
                    "pdftotext found no text layer",
                    "Likely a scanned PDF. Set [ocr].enabled = true in config.toml to fall back to OCR, or pass --force-ocr.",
                )
            };
            return Err(IngestError::PdfExtract {
                source: format_threshold_failure(
                    prefix,
                    bad,
                    &pages,
                    pages.len(),
                    ocr.min_decodable_ratio,
                    initial_scripts,
                    Some(hint),
                )
                .into(),
            });
        }

        tracing::info!(
            ocr_pages = bad.len(),
            garbled = selection.garbled,
            sparse = selection.sparse,
            forced = opts.force_ocr,
            total_pages,
            language = %lang_spec,
            "pdf: running OCR on {} pages",
            bad.len(),
        );

        let installed = installed_tesseract_langs(cancel).await;
        let lang = if lang_spec == AUTO_LANGUAGE {
            detect_ocr_language(path, bad, ocr, installed.as_deref(), cancel).await?
        } else {
            check_langs_installed(&lang_spec, installed.as_deref())?;
            lang_spec.clone()
        };
        let scripts = ScriptSet::from_tesseract_langs(&lang);

        let bad_total = bad.len() as u32;
        for (k, &idx) in bad.iter().enumerate() {
            let page_num = (idx + 1) as u32;
            on_progress(ExtractProgress::Ocr {
                current: k as u32 + 1,
                total: bad_total,
            });
            let ocr_text = ocr_page(path, page_num, ocr, &lang, scripts, cancel).await?;
            tracing::debug!(
                page = page_num,
                pre_score = decodability_score(&pages[idx], scripts),
                post_score = decodability_score(&ocr_text, scripts),
                ocr_chars = ocr_text.chars().count(),
                "pdf: ocr replaced page",
            );
            pages[idx] = ocr_text;
        }

        let still_bad: Vec<usize> = bad
            .iter()
            .copied()
            .filter(|&i| needs_ocr(&pages[i], ocr, scripts))
            .collect();
        if within_undecodable_tolerance(still_bad.len(), pages.len(), ocr) {
            for &i in &still_bad {
                tracing::warn!(
                    page = i + 1,
                    score = decodability_score(&pages[i], scripts),
                    threshold = ocr.min_decodable_ratio,
                    language = %lang,
                    "pdf: keeping best-effort OCR text for undecodable page",
                );
            }
        } else {
            return Err(IngestError::PdfExtract {
                source: format_threshold_failure(
                    "OCR could not produce decodable text",
                    &still_bad,
                    &pages,
                    pages.len(),
                    ocr.min_decodable_ratio,
                    scripts,
                    Some(&format!(
                        "Try increasing [ocr].dpi (currently {}) or a different tesseract language (currently \"{}\"; e.g. --ocr-language jpn_vert+jpn for vertical Japanese).",
                        ocr.dpi, lang,
                    )),
                )
                .into(),
            });
        }
    }

    if pages.iter().all(|p| p.chars().all(char::is_whitespace)) {
        return Err(IngestError::PdfExtract {
            source: format!(
                "no text extracted from {} pages{}",
                pages.len(),
                if bad.is_empty() {
                    "; try --force-ocr if this is a scanned PDF"
                } else {
                    " even after OCR; check the PDF renders, or try a different --ocr-language"
                },
            )
            .into(),
        });
    }

    let mut full_text = String::new();
    let mut page_offsets = Vec::with_capacity(pages.len());
    let mut char_cursor: usize = 0;

    for (i, page_text) in pages.iter().enumerate() {
        let page_num = (i + 1) as u32;
        if i > 0 {
            full_text.push_str(PAGE_SEPARATOR);
            char_cursor += PAGE_SEPARATOR.chars().count();
        }
        let byte_start = full_text.len();
        let char_start = char_cursor;
        full_text.push_str(page_text);
        let byte_end = full_text.len();
        let page_chars = page_text.chars().count();
        char_cursor += page_chars;
        let char_end = char_cursor;

        page_offsets.push(PageOffset {
            page_number: page_num,
            byte_start,
            byte_end,
            char_start,
            char_end,
        });
    }

    tracing::info!(
        total_pages,
        total_bytes = full_text.len(),
        ocr_pages = selection.pages.len(),
        total_secs = started.elapsed().as_secs_f64(),
        "pdf: extraction complete",
    );

    Ok(PdfText {
        full_text,
        page_offsets,
    })
}

/// Rasterize one page to a PNG via pdftoppm. The returned `TempDir` owns the
/// PNG; drop it to clean up, so only one page image is on disk at a time.
async fn rasterize_page(
    pdf_path: &Path,
    page_number: u32,
    dpi: u32,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<(tempfile::TempDir, PathBuf), IngestError> {
    let tmp = tempfile::tempdir().map_err(|e| IngestError::PdfExtract {
        source: format!("creating temp dir for OCR: {e}").into(),
    })?;
    let stem = tmp.path().join("page");
    let page_arg = page_number.to_string();
    let dpi_arg = dpi.to_string();

    run_cmd(
        "pdftoppm",
        &[
            OsStr::new("-q"),
            OsStr::new("-f"),
            OsStr::new(&page_arg),
            OsStr::new("-l"),
            OsStr::new(&page_arg),
            OsStr::new("-r"),
            OsStr::new(&dpi_arg),
            OsStr::new("-png"),
            pdf_path.as_os_str(),
            stem.as_os_str(),
        ],
        "poppler-utils",
        cancel,
    )
    .await?;

    // pdftoppm appends `-NNN.png` (with variable zero-padding based on the
    // document's total page count) to the stem. Find the single PNG it
    // produced rather than guessing the padding.
    let png_path = find_single_png(tmp.path()).map_err(|e| IngestError::PdfExtract {
        source: format!("locating pdftoppm output for page {page_number}: {e}").into(),
    })?;
    Ok((tmp, png_path))
}

const TESSERACT_HINT: &str = "tesseract; on Arch: pacman -S tesseract, on Debian/Ubuntu: apt install tesseract-ocr";

/// OCR a single page with tesseract using the resolved language spec `lang`.
/// For CJK scripts, strips the spaces tesseract inserts between characters.
async fn ocr_page(
    pdf_path: &Path,
    page_number: u32,
    ocr: &OcrConfig,
    lang: &str,
    scripts: ScriptSet,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, IngestError> {
    let (_tmp, png_path) = rasterize_page(pdf_path, page_number, ocr.dpi, cancel).await?;

    let psm_arg = ocr.psm.to_string();
    let stdout = run_cmd(
        "tesseract",
        &[
            png_path.as_os_str(),
            OsStr::new("-"),
            OsStr::new("-l"),
            OsStr::new(lang),
            OsStr::new("--psm"),
            OsStr::new(&psm_arg),
        ],
        TESSERACT_HINT,
        cancel,
    )
    .await?;

    let text = String::from_utf8_lossy(&stdout).into_owned();
    Ok(if scripts.any_cjk() {
        collapse_cjk_spaces(&text)
    } else {
        text
    })
}

/// Installed tesseract language packs, or `None` if `--list-langs` couldn't
/// be run/parsed (in which case callers skip the pre-check and let tesseract
/// itself report a missing pack).
async fn installed_tesseract_langs(cancel: Option<&watch::Receiver<bool>>) -> Option<Vec<String>> {
    let out = run_cmd_output("tesseract", &[OsStr::new("--list-langs")], TESSERACT_HINT, cancel)
        .await
        .ok()?;
    // Older tesseract versions print the list on stderr.
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let langs = parse_list_langs(&text);
    (!langs.is_empty()).then_some(langs)
}

fn parse_list_langs(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("List of available languages"))
        .filter(|l| l.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .map(str::to_string)
        .collect()
}

/// Error out with an install hint if any `+`-joined code in `spec` isn't an
/// installed pack. No-op when the installed list is unknown.
fn check_langs_installed(spec: &str, installed: Option<&[String]>) -> Result<(), IngestError> {
    let Some(installed) = installed else {
        return Ok(());
    };
    let missing: Vec<&str> = spec
        .split('+')
        .map(str::trim)
        .filter(|c| !c.is_empty() && !installed.iter().any(|i| i == c))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let pkgs: Vec<String> = missing.iter().map(|c| format!("tesseract-data-{c}")).collect();
    Err(IngestError::PdfExtract {
        source: format!(
            "tesseract language pack(s) not installed: {} (on Arch: pacman -S {}; on Debian/Ubuntu the packages are tesseract-ocr-<code>)",
            missing.join(", "),
            pkgs.join(" "),
        )
        .into(),
    })
}

/// Resolve `"auto"` to a concrete tesseract spec by running OSD
/// (`--psm 0`) on the first few candidate pages until one reports a script.
async fn detect_ocr_language(
    pdf_path: &Path,
    candidates: &[usize],
    ocr: &OcrConfig,
    installed: Option<&[String]>,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, IngestError> {
    if let Some(langs) = installed
        && !langs.iter().any(|l| l == "osd")
    {
        return Err(IngestError::PdfExtract {
            source: "[ocr].language = \"auto\" needs tesseract's osd.traineddata \
                     (on Arch: pacman -S tesseract-data-osd), or pass --ocr-language <codes>"
                .into(),
        });
    }

    let mut last_err: Option<String> = None;
    for &idx in candidates.iter().take(OSD_MAX_ATTEMPTS) {
        let page_number = (idx + 1) as u32;
        let (_tmp, png) = rasterize_page(pdf_path, page_number, ocr.dpi, cancel).await?;
        let out = match run_cmd_output(
            "tesseract",
            &[png.as_os_str(), OsStr::new("-"), OsStr::new("--psm"), OsStr::new("0")],
            TESSERACT_HINT,
            cancel,
        )
        .await
        {
            Ok(out) => out,
            Err(IngestError::Cancelled) => return Err(IngestError::Cancelled),
            Err(e) => {
                tracing::debug!(page = page_number, error = %e, "pdf: OSD failed on page");
                last_err = Some(e.to_string());
                continue;
            }
        };
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let Some(script) = parse_osd_script(&text) else {
            last_err = Some(format!("no script reported for page {page_number}"));
            continue;
        };
        let spec = tesseract_spec_for_script(&script, installed).ok_or_else(|| {
            IngestError::PdfExtract {
                source: format!(
                    "OCR auto-detection found script \"{script}\", which has no built-in \
                     language mapping; pass --ocr-language <codes>"
                )
                .into(),
            }
        })?;
        check_langs_installed(&spec, installed)?;
        tracing::info!(page = page_number, script = %script, language = %spec, "pdf: OSD picked OCR language");
        return Ok(spec);
    }
    Err(IngestError::PdfExtract {
        source: format!(
            "OCR language auto-detection failed on the first {} candidate pages ({}); pass --ocr-language <codes>",
            candidates.len().min(OSD_MAX_ATTEMPTS),
            last_err.unwrap_or_else(|| "no candidates".to_string()),
        )
        .into(),
    })
}

/// Pull the `Script: <name>` value out of tesseract OSD output.
fn parse_osd_script(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Script:")?.trim();
        (!rest.is_empty()).then(|| rest.to_string())
    })
}

/// Map an OSD script name to a tesseract language spec. Adds vertical
/// variants when installed (vertical layout is common in Japanese/Chinese
/// print) and `eng` for the Latin text that shows up in most documents.
fn tesseract_spec_for_script(script: &str, installed: Option<&[String]>) -> Option<String> {
    let has = |code: &str| installed.is_none_or(|l| l.iter().any(|i| i == code));
    let primary: &[&str] = match script {
        "Japanese" | "Katakana" | "Hiragana" => &["jpn", "jpn_vert"],
        "Han" | "HanS" => &["chi_sim", "chi_sim_vert"],
        "HanT" => &["chi_tra", "chi_tra_vert"],
        "Hangul" | "Korean" => &["kor", "kor_vert"],
        "Cyrillic" => &["rus"],
        "Greek" => &["ell"],
        "Arabic" => &["ara"],
        "Hebrew" => &["heb"],
        "Devanagari" => &["hin"],
        "Thai" => &["tha"],
        "Latin" => &["eng"],
        _ => return None,
    };
    // The base pack is required (checked by the caller); optional variants
    // only when present.
    let mut codes: Vec<&str> = vec![primary[0]];
    codes.extend(primary[1..].iter().copied().filter(|c| installed.is_some() && has(c)));
    if !codes.contains(&"eng") && has("eng") {
        codes.push("eng");
    }
    Some(codes.join("+"))
}

fn find_single_png(dir: &Path) -> std::io::Result<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("png") {
            if found.is_some() {
                return Err(std::io::Error::other(
                    "expected exactly one PNG in temp dir",
                ));
            }
            found = Some(p);
        }
    }
    found.ok_or_else(|| std::io::Error::other("no PNG produced"))
}

/// Run a binary and return its stdout. Maps a missing binary to a friendly
/// "install <hint>" message. On non-zero exit, includes stderr in the error.
///
/// If `cancel` fires before the child exits, the child is killed via
/// `kill_on_drop` (we drop the `wait_with_output` future, which owns the
/// `Child`) and `IngestError::Cancelled` is returned.
async fn run_cmd(
    name: &'static str,
    args: &[&OsStr],
    install_hint: &str,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Vec<u8>, IngestError> {
    Ok(run_cmd_output(name, args, install_hint, cancel).await?.stdout)
}

/// `run_cmd`, but returns the full output (stdout + stderr) on success.
async fn run_cmd_output(
    name: &'static str,
    args: &[&OsStr],
    install_hint: &str,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<std::process::Output, IngestError> {
    if let Some(rx) = cancel {
        if *rx.borrow() {
            return Err(IngestError::Cancelled);
        }
    }
    let owned: Vec<OsString> = args.iter().map(|a| a.to_os_string()).collect();
    let child = Command::new(name)
        .args(&owned)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| spawn_err(name, install_hint, e))?;

    let wait_fut = child.wait_with_output();
    let output = match cancel {
        Some(rx) => {
            let mut rx = rx.clone();
            tokio::select! {
                res = wait_fut => res.map_err(|e| IngestError::PdfExtract {
                    source: format!("waiting on {name}: {e}").into(),
                })?,
                _ = rx.changed() => {
                    // Either cancel fired or the sender was dropped (server
                    // shutdown); either way, kill the child (drop did so) and
                    // bail.
                    return Err(IngestError::Cancelled);
                }
            }
        }
        None => wait_fut.await.map_err(|e| IngestError::PdfExtract {
            source: format!("waiting on {name}: {e}").into(),
        })?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("exited with status {}", output.status)
        } else {
            stderr
        };
        return Err(IngestError::PdfExtract {
            source: format!("{name}: {detail}").into(),
        });
    }
    Ok(output)
}

fn spawn_err(name: &'static str, install_hint: &str, e: std::io::Error) -> IngestError {
    let msg = if e.kind() == std::io::ErrorKind::NotFound {
        format!("{name} binary not found in PATH (install {install_hint})")
    } else {
        format!("spawning {name}: {e}")
    };
    IngestError::PdfExtract { source: msg.into() }
}

/// Decodability score: ratio of "text-like" chars to non-whitespace chars.
/// Returns 1.0 for very short pages (so they aren't flagged on noise alone).
///
/// Garbled CID-Identity-H output sits in the U+00C0–U+00FF Latin-1 supplement
/// range (Ê Ì Ã Õ Ü À), which is *not* in our text-like allowlist — those
/// pages score near 0.0. Clean English scores 0.95+. `scripts` widens the
/// allowlist to the CJK/Hangul ranges the configured OCR language implies;
/// it deliberately never admits Latin-1, so the garble signal survives.
pub fn decodability_score(text: &str, scripts: ScriptSet) -> f32 {
    let mut text_like: u32 = 0;
    let mut total: u32 = 0;
    for c in text.chars() {
        if c.is_whitespace() {
            continue;
        }
        total += 1;
        if is_text_like(c) || scripts.allows(c) {
            text_like += 1;
        }
    }
    if total == 0 {
        return 1.0;
    }
    text_like as f32 / total as f32
}

fn is_text_like(c: char) -> bool {
    if c.is_ascii_alphanumeric() {
        return true;
    }
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}'
            | '<' | '>' | '/' | '-' | '_' | '&' | '%' | '$' | '#' | '@' | '*' | '+' | '='
            | '|' | '\\' | '^' | '`' | '~'
            // typographic punctuation that legitimately appears in clean text
            | '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' | '\u{2013}' | '\u{2014}'
            | '\u{2026}'
    )
}

fn needs_ocr(text: &str, ocr: &OcrConfig, scripts: ScriptSet) -> bool {
    if is_sparse(text, ocr) {
        return false;
    }
    decodability_score(text, scripts) < ocr.min_decodable_ratio
}

/// Too few non-whitespace chars to score reliably. Also the sparse-page
/// trigger's definition of a near-empty page.
fn is_sparse(text: &str, ocr: &OcrConfig) -> bool {
    text.chars().filter(|c| !c.is_whitespace()).count() < ocr.min_chars_for_check
}

/// Which pages to OCR, plus why (for logging and error wording).
#[derive(Debug, Default, PartialEq, Eq)]
struct OcrSelection {
    /// 0-indexed, ascending.
    pages: Vec<usize>,
    garbled: usize,
    sparse: usize,
}

/// Pick pages to OCR:
/// - `force`: every page.
/// - garbled pages (`needs_ocr`).
/// - sparse pages, when the document has at least `ocr.sparse_min_pages`
///   pages and at least `ocr.sparse_page_ratio` of them are sparse, or when
///   every page is entirely empty (a short scan with no text layer at all).
fn select_ocr_pages(
    pages: &[String],
    ocr: &OcrConfig,
    scripts: ScriptSet,
    force: bool,
) -> OcrSelection {
    if force {
        return OcrSelection {
            pages: (0..pages.len()).collect(),
            ..Default::default()
        };
    }
    let garbled: Vec<usize> = (0..pages.len())
        .filter(|&i| needs_ocr(&pages[i], ocr, scripts))
        .collect();
    let sparse: Vec<usize> = (0..pages.len())
        .filter(|&i| is_sparse(&pages[i], ocr))
        .collect();
    let all_empty = !pages.is_empty()
        && pages.iter().all(|p| p.chars().all(char::is_whitespace));
    let mostly_sparse = pages.len() >= ocr.sparse_min_pages.max(1)
        && sparse.len() as f32 / pages.len() as f32 >= ocr.sparse_page_ratio;
    let take_sparse = all_empty || mostly_sparse;

    let mut out = garbled.clone();
    if take_sparse {
        out.extend(sparse.iter().copied());
        out.sort_unstable();
        out.dedup();
    }
    OcrSelection {
        pages: out,
        garbled: garbled.len(),
        sparse: if take_sparse { sparse.len() } else { 0 },
    }
}

/// Non-ASCII scripts that count as legitimate text for the decodability
/// score, derived from a tesseract language spec.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScriptSet {
    japanese: bool,
    chinese: bool,
    korean: bool,
}

impl ScriptSet {
    /// Parse a `+`-joined tesseract spec (`"jpn+eng"`, `"chi_sim_vert"`).
    /// `"auto"` admits every script we know, since the real language isn't
    /// resolved until OCR runs.
    pub fn from_tesseract_langs(spec: &str) -> Self {
        let mut set = ScriptSet::default();
        for code in spec.split('+').map(str::trim) {
            if code == AUTO_LANGUAGE {
                return ScriptSet {
                    japanese: true,
                    chinese: true,
                    korean: true,
                };
            }
            if code.starts_with("jpn") {
                set.japanese = true;
            } else if code.starts_with("chi_") {
                set.chinese = true;
            } else if code.starts_with("kor") {
                set.korean = true;
            }
        }
        set
    }

    fn any_cjk(self) -> bool {
        self.japanese || self.chinese || self.korean
    }

    fn allows(self, c: char) -> bool {
        (self.japanese && is_kana(c))
            || ((self.japanese || self.chinese) && is_han(c))
            || (self.korean && is_hangul(c))
            || (self.any_cjk() && is_cjk_punct_or_fullwidth(c))
    }
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}')
}

fn is_han(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2A6DF}'
    )
}

fn is_hangul(c: char) -> bool {
    matches!(
        c,
        '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}'
    )
}

/// CJK symbols/punctuation (、。「」々〜…) and full/half-width forms
/// (！？（）, full-width Latin, half-width katakana).
fn is_cjk_punct_or_fullwidth(c: char) -> bool {
    matches!(c, '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}')
}

fn is_cjk(c: char) -> bool {
    is_kana(c) || is_han(c) || is_hangul(c) || is_cjk_punct_or_fullwidth(c)
}

/// Tesseract's CJK models emit a space between most characters ("日 本 語").
/// Remove runs of spaces/tabs whose neighbours on both sides are CJK; keep
/// newlines (they carry paragraph structure the chunker uses) and spaces
/// next to Latin text.
fn collapse_cjk_spaces(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' || c == '\t' {
            let mut j = i;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                j += 1;
            }
            let prev_cjk = i > 0 && is_cjk(chars[i - 1]);
            let next_cjk = j < chars.len() && is_cjk(chars[j]);
            if !(prev_cjk && next_cjk) {
                out.extend(&chars[i..j]);
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Whether `still_bad` undecodable-after-OCR pages out of `total_pages` are
/// few enough (per `ocr.max_undecodable_page_ratio`) to keep their
/// best-effort OCR text instead of failing the ingest.
fn within_undecodable_tolerance(still_bad: usize, total_pages: usize, ocr: &OcrConfig) -> bool {
    still_bad == 0
        || (total_pages > 0
            && still_bad as f32 / total_pages as f32 <= ocr.max_undecodable_page_ratio)
}

/// Format a "X of N pages failed the threshold" error string. Lists up to
/// three worst offenders (by score) with their page numbers and scores so
/// the user can spot-check.
fn format_threshold_failure(
    prefix: &str,
    bad_indices: &[usize],
    pages: &[String],
    total_pages: usize,
    threshold: f32,
    scripts: ScriptSet,
    suffix: Option<&str>,
) -> String {
    let mut scored: Vec<(usize, f32)> = bad_indices
        .iter()
        .map(|&i| (i, decodability_score(&pages[i], scripts)))
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let worst: Vec<String> = scored
        .iter()
        .take(3)
        .map(|(i, s)| format!("page {} score {:.2}", i + 1, s))
        .collect();
    let mut msg = format!(
        "{prefix} for {} of {} pages (worst: {}; threshold {:.2}).",
        bad_indices.len(),
        total_pages,
        worst.join(", "),
        threshold,
    );
    if let Some(suffix) = suffix {
        msg.push(' ');
        msg.push_str(suffix);
    }
    msg
}

/// Find the 1-indexed page that contains the given byte offset. The offset
/// is treated as inclusive of `byte_start` and exclusive of `byte_end`. The
/// PAGE_SEPARATOR byte sits between pages and is reported as belonging to
/// the *preceding* page (matches user intuition: a chunk spanning pages 2-3
/// gets `page_first = 2` even if its first byte is the separator after p1).
pub fn page_for_byte(offsets: &[PageOffset], byte_offset: usize) -> Option<u32> {
    let idx = offsets
        .binary_search_by(|p| {
            if byte_offset < p.byte_start {
                std::cmp::Ordering::Greater
            } else if byte_offset >= p.byte_end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()?;
    Some(offsets[idx].page_number)
}

/// Same as `page_for_byte`, but if the offset falls in a separator gap it
/// returns the page just before it (last page whose `byte_end <= offset`).
pub fn page_for_byte_or_preceding(offsets: &[PageOffset], byte_offset: usize) -> Option<u32> {
    if let Some(p) = page_for_byte(offsets, byte_offset) {
        return Some(p);
    }
    offsets
        .iter()
        .rev()
        .find(|p| p.byte_end <= byte_offset)
        .map(|p| p.page_number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodability_clean_english_above_threshold() {
        let text = "The quick brown fox jumps over the lazy dog. \
                    Pack my box with five dozen liquor jugs. \
                    She sells seashells by the seashore.";
        let score = decodability_score(text, ScriptSet::default());
        assert!(score > 0.95, "clean English scored {score}");
    }

    #[test]
    fn decodability_garbled_cid_below_threshold() {
        // Sample lifted from real Exalted 2E pdftotext output — CID font with
        // no ToUnicode CMap. Real-world scores hover around 0.4-0.5 because
        // some glyph IDs happen to land on ASCII letters; the default 0.7
        // threshold still flags them while clean English (0.95+) sails past.
        let text = "ÊivviVÌÛiÊÜ iÌ ÀÜ7 i>ÊÌ ÀÜÜi>«ÃÊÕÃi`Ê >Ì > VL>Ì]Ê \
                    Ì iÊ V >À>VÌiÀÊ ÕÃÊ Ê ÕÃiÊ iÀÊ iiiÊ LÌÞ°Ê";
        let score = decodability_score(text, ScriptSet::default());
        assert!(
            score < 0.7,
            "garbled CID scored {score} (must be below 0.7 default threshold)"
        );
    }

    #[test]
    fn decodability_empty_text() {
        assert_eq!(decodability_score("", ScriptSet::default()), 1.0);
    }

    #[test]
    fn decodability_table_of_numbers_high() {
        let text = "123 456\n789 012\n345 678\n";
        let score = decodability_score(text, ScriptSet::default());
        assert!(score > 0.7, "numeric table scored {score}");
    }

    #[test]
    fn needs_ocr_skips_short_pages() {
        let cfg = OcrConfig {
            enabled: true,
            min_decodable_ratio: 0.7,
            min_chars_for_check: 40,
            dpi: 300,
            language: "eng".into(),
            psm: 3,
            sparse_page_ratio: 0.5,
            sparse_min_pages: 3,
            max_undecodable_page_ratio: 0.05,
        };
        // Five non-whitespace chars — way under the threshold — but page is
        // too short to trust the score, so we don't flag it.
        assert!(!needs_ocr("ÊÌÃÕÜ", &cfg, ScriptSet::default()));
    }

    #[test]
    fn needs_ocr_flags_garbled_long_page() {
        let cfg = OcrConfig {
            enabled: true,
            min_decodable_ratio: 0.7,
            min_chars_for_check: 40,
            dpi: 300,
            language: "eng".into(),
            psm: 3,
            sparse_page_ratio: 0.5,
            sparse_min_pages: 3,
            max_undecodable_page_ratio: 0.05,
        };
        let text = "ÊivviVÌÛiÊÜ iÌ ÀÜ7 i>ÊÌ ÀÜÜi>«ÃÊÕÃi`Ê >Ì > VL>Ì]Ê \
                    Ì iÊ V >À>VÌiÀÊ ÕÃÌÊ ÕÃiÊ iÀÊ iiiÊ LÌÞ°Ê \
                    ÕÃ >Ê>ÀÌ>>ÀÌÃÊÜi>«Ã iÊ>ÞÊÃÕLÃÌÌÕÌiÊ iÀÊ>ÀÌ>ÊÀÌÃÊvÀ";
        assert!(needs_ocr(text, &cfg, ScriptSet::default()));
    }

    #[test]
    fn needs_ocr_passes_clean_english() {
        let cfg = OcrConfig {
            enabled: true,
            min_decodable_ratio: 0.7,
            min_chars_for_check: 40,
            dpi: 300,
            language: "eng".into(),
            psm: 3,
            sparse_page_ratio: 0.5,
            sparse_min_pages: 3,
            max_undecodable_page_ratio: 0.05,
        };
        let text = "The quick brown fox jumps over the lazy dog. \
                    Pack my box with five dozen liquor jugs. \
                    She sells seashells by the seashore.";
        assert!(!needs_ocr(text, &cfg, ScriptSet::default()));
    }

    #[test]
    fn undecodable_tolerance_allows_a_stray_page() {
        let cfg = test_cfg();
        assert!(within_undecodable_tolerance(0, 264, &cfg));
        // One bad back-cover page out of 264 is well under 5%.
        assert!(within_undecodable_tolerance(1, 264, &cfg));
        assert!(within_undecodable_tolerance(5, 100, &cfg));
    }

    #[test]
    fn undecodable_tolerance_rejects_wrong_language_runs() {
        let cfg = test_cfg();
        assert!(!within_undecodable_tolerance(6, 100, &cfg));
        assert!(!within_undecodable_tolerance(231, 248, &cfg));
        // Small docs: one bad page out of three is a real failure.
        assert!(!within_undecodable_tolerance(1, 3, &cfg));
    }

    #[test]
    fn undecodable_tolerance_zero_is_strict() {
        let cfg = OcrConfig {
            max_undecodable_page_ratio: 0.0,
            ..test_cfg()
        };
        assert!(within_undecodable_tolerance(0, 264, &cfg));
        assert!(!within_undecodable_tolerance(1, 264, &cfg));
    }

    fn test_cfg() -> OcrConfig {
        OcrConfig {
            enabled: true,
            min_decodable_ratio: 0.7,
            min_chars_for_check: 40,
            dpi: 300,
            language: "jpn".into(),
            psm: 3,
            sparse_page_ratio: 0.5,
            sparse_min_pages: 3,
            max_undecodable_page_ratio: 0.05,
        }
    }

    const JAPANESE: &str = "吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。\
        何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。\
        「吾輩はここで始めて人間というものを見た。」";

    const ENGLISH: &str = "The quick brown fox jumps over the lazy dog. \
        Pack my box with five dozen liquor jugs. She sells seashells by the seashore.";

    const GARBLED: &str = "ÊivviVÌÛiÊÜ iÌ ÀÜ7 i>ÊÌ ÀÜÜi>«ÃÊÕÃi`Ê >Ì > VL>Ì]Ê \
        Ì iÊ V >À>VÌiÀÊ ÕÃÌÊ ÕÃiÊ iÀÊ iiiÊ LÌÞ°Ê";

    #[test]
    fn script_set_parses_tesseract_specs() {
        let jpn = ScriptSet::from_tesseract_langs("jpn_vert+jpn+eng");
        assert!(jpn.japanese && !jpn.chinese && !jpn.korean);
        let eng = ScriptSet::from_tesseract_langs("eng");
        assert!(!eng.any_cjk());
        let auto = ScriptSet::from_tesseract_langs("auto");
        assert!(auto.japanese && auto.chinese && auto.korean);
        assert!(ScriptSet::from_tesseract_langs("chi_tra").chinese);
        assert!(ScriptSet::from_tesseract_langs("kor").korean);
    }

    #[test]
    fn japanese_passes_with_jpn_and_fails_with_eng() {
        let cfg = test_cfg();
        let jpn = ScriptSet::from_tesseract_langs("jpn");
        assert!(decodability_score(JAPANESE, jpn) > 0.95);
        assert!(!needs_ocr(JAPANESE, &cfg, jpn));
        assert!(needs_ocr(JAPANESE, &cfg, ScriptSet::default()));
    }

    #[test]
    fn latin1_garbage_still_fails_with_cjk_scripts() {
        let cfg = test_cfg();
        let auto = ScriptSet::from_tesseract_langs("auto");
        assert!(needs_ocr(GARBLED, &cfg, auto));
    }

    #[test]
    fn hangul_only_allowed_for_korean() {
        let text = "대한민국은 민주공화국이다. 대한민국의 주권은 국민에게 있고, 모든 권력은 국민으로부터 나온다.";
        assert!(decodability_score(text, ScriptSet::from_tesseract_langs("kor")) > 0.95);
        assert!(decodability_score(text, ScriptSet::from_tesseract_langs("jpn")) < 0.2);
    }

    fn pages(v: &[&str]) -> Vec<String> {
        v.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn select_force_takes_every_page() {
        let sel = select_ocr_pages(&pages(&[ENGLISH, ENGLISH]), &test_cfg(), ScriptSet::default(), true);
        assert_eq!(sel.pages, vec![0, 1]);
    }

    #[test]
    fn select_takes_sparse_pages_when_mostly_sparse() {
        let p = pages(&[ENGLISH, "", "  \n", "12", ENGLISH]);
        let sel = select_ocr_pages(&p, &test_cfg(), ScriptSet::default(), false);
        assert_eq!(sel.pages, vec![1, 2, 3]);
        assert_eq!(sel.sparse, 3);
        assert_eq!(sel.garbled, 0);
    }

    #[test]
    fn select_ignores_occasional_blank_pages() {
        let p = pages(&[ENGLISH, "", ENGLISH, ENGLISH]);
        let sel = select_ocr_pages(&p, &test_cfg(), ScriptSet::default(), false);
        assert!(sel.pages.is_empty());
    }

    #[test]
    fn select_short_fully_empty_doc_takes_all_pages() {
        // Below sparse_min_pages, but there's no text layer at all.
        let p = pages(&["", "\n"]);
        let sel = select_ocr_pages(&p, &test_cfg(), ScriptSet::default(), false);
        assert_eq!(sel.pages, vec![0, 1]);
    }

    #[test]
    fn select_combines_garbled_and_sparse() {
        let p = pages(&[GARBLED, "", "", ENGLISH]);
        let sel = select_ocr_pages(&p, &test_cfg(), ScriptSet::default(), false);
        assert_eq!(sel.pages, vec![0, 1, 2]);
        assert_eq!(sel.garbled, 1);
        assert_eq!(sel.sparse, 2);
    }

    #[test]
    fn collapse_cjk_spaces_cases() {
        assert_eq!(collapse_cjk_spaces("日 本 語"), "日本語");
        assert_eq!(collapse_cjk_spaces("日本 ABC"), "日本 ABC");
        assert_eq!(collapse_cjk_spaces("日 本\n語 で す 。"), "日本\n語です。");
        assert_eq!(collapse_cjk_spaces("one two"), "one two");
        assert_eq!(collapse_cjk_spaces("「 吾 輩 」\t は"), "「吾輩」は");
    }

    #[test]
    fn parse_osd_script_reads_script_line() {
        let out = "Page number: 0\nOrientation in degrees: 0\nRotate: 0\n\
                   Orientation confidence: 2.11\nScript: Japanese\nScript confidence: 1.67\n";
        assert_eq!(parse_osd_script(out).as_deref(), Some("Japanese"));
        assert_eq!(parse_osd_script("nothing here"), None);
    }

    #[test]
    fn spec_for_script_adds_installed_variants_and_eng() {
        let installed: Vec<String> = ["eng", "jpn", "jpn_vert", "osd"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            tesseract_spec_for_script("Japanese", Some(&installed)).as_deref(),
            Some("jpn+jpn_vert+eng")
        );
        let minimal: Vec<String> = ["jpn"].iter().map(|s| s.to_string()).collect();
        assert_eq!(tesseract_spec_for_script("Japanese", Some(&minimal)).as_deref(), Some("jpn"));
        assert_eq!(tesseract_spec_for_script("Latin", None).as_deref(), Some("eng"));
        assert_eq!(tesseract_spec_for_script("Klingon", None), None);
    }

    #[test]
    fn list_langs_parsing_and_install_check() {
        let out = "List of available languages in \"/usr/share/tessdata/\" (3):\neng\njpn\nosd\n";
        let langs = parse_list_langs(out);
        assert_eq!(langs, vec!["eng", "jpn", "osd"]);
        assert!(check_langs_installed("jpn+eng", Some(&langs)).is_ok());
        let err = check_langs_installed("jpn_vert+jpn", Some(&langs)).unwrap_err().to_string();
        assert!(err.contains("tesseract-data-jpn_vert"), "{err}");
        assert!(check_langs_installed("kor", None).is_ok());
    }
}
