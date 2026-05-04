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

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

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

/// Extract canonical PDF text. First runs pdftotext over the whole document,
/// then per-page scores the output with `decodability_score`. Pages below
/// `ocr.min_decodable_ratio` get re-extracted via `pdftoppm` + `tesseract`
/// (one page at a time). After OCR, pages still below the threshold are
/// fatal: we return `IngestError::PdfExtract` so the caller can refuse the
/// ingest rather than indexing garbage.
pub fn extract<F>(
    path: &Path,
    ocr: &OcrConfig,
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
    )?;

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

    let bad: Vec<usize> = pages
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            if needs_ocr(p, ocr) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if !bad.is_empty() {
        if !ocr.enabled {
            return Err(IngestError::PdfExtract {
                source: format_threshold_failure(
                    "pdftotext produced unreadable output",
                    &bad,
                    &pages,
                    pages.len(),
                    ocr.min_decodable_ratio,
                    Some("Likely a CID font without ToUnicode CMap. Set [ocr].enabled = true in config.toml to fall back to OCR."),
                )
                .into(),
            });
        }

        tracing::info!(
            bad_pages = bad.len(),
            total_pages,
            "pdf: pdftotext output unreadable for {} pages, running OCR",
            bad.len(),
        );

        let bad_total = bad.len() as u32;
        for (k, &idx) in bad.iter().enumerate() {
            let page_num = (idx + 1) as u32;
            on_progress(ExtractProgress::Ocr {
                current: k as u32 + 1,
                total: bad_total,
            });
            let ocr_text = ocr_page(path, page_num, ocr)?;
            tracing::debug!(
                page = page_num,
                pre_score = decodability_score(&pages[idx]),
                post_score = decodability_score(&ocr_text),
                ocr_chars = ocr_text.chars().count(),
                "pdf: ocr replaced page",
            );
            pages[idx] = ocr_text;
        }

        let still_bad: Vec<usize> = bad
            .iter()
            .copied()
            .filter(|&i| needs_ocr(&pages[i], ocr))
            .collect();
        if !still_bad.is_empty() {
            return Err(IngestError::PdfExtract {
                source: format_threshold_failure(
                    "OCR could not produce decodable text",
                    &still_bad,
                    &pages,
                    pages.len(),
                    ocr.min_decodable_ratio,
                    Some(&format!(
                        "Try increasing [ocr].dpi (currently {}) or installing additional tesseract language packs (currently \"{}\").",
                        ocr.dpi, ocr.language,
                    )),
                )
                .into(),
            });
        }
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
        ocr_pages = bad.len(),
        total_secs = started.elapsed().as_secs_f64(),
        "pdf: extraction complete",
    );

    Ok(PdfText {
        full_text,
        page_offsets,
    })
}

/// OCR a single page: rasterize via pdftoppm into a temp dir, then run
/// tesseract on the resulting PNG. The temp dir is dropped (and cleaned up)
/// at the end of this function, so only one PNG is ever on disk per call.
fn ocr_page(pdf_path: &Path, page_number: u32, ocr: &OcrConfig) -> Result<String, IngestError> {
    let tmp = tempfile::tempdir().map_err(|e| IngestError::PdfExtract {
        source: format!("creating temp dir for OCR: {e}").into(),
    })?;
    let stem = tmp.path().join("page");
    let page_arg = page_number.to_string();
    let dpi_arg = ocr.dpi.to_string();

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
    )?;

    // pdftoppm appends `-NNN.png` (with variable zero-padding based on the
    // document's total page count) to the stem. Find the single PNG it
    // produced rather than guessing the padding.
    let png_path = find_single_png(tmp.path()).map_err(|e| IngestError::PdfExtract {
        source: format!("locating pdftoppm output for page {page_number}: {e}").into(),
    })?;

    let psm_arg = ocr.psm.to_string();
    let stdout = run_cmd(
        "tesseract",
        &[
            png_path.as_os_str(),
            OsStr::new("-"),
            OsStr::new("-l"),
            OsStr::new(&ocr.language),
            OsStr::new("--psm"),
            OsStr::new(&psm_arg),
        ],
        "tesseract-ocr; on Debian/Ubuntu: apt install tesseract-ocr",
    )?;

    Ok(String::from_utf8_lossy(&stdout).into_owned())
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
fn run_cmd(name: &'static str, args: &[&OsStr], install_hint: &str) -> Result<Vec<u8>, IngestError> {
    let owned: Vec<OsString> = args.iter().map(|a| a.to_os_string()).collect();
    let output = Command::new(name)
        .args(&owned)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| spawn_err(name, install_hint, e))?;

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
    Ok(output.stdout)
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
/// pages score near 0.0. Clean English scores 0.95+.
pub fn decodability_score(text: &str) -> f32 {
    let mut text_like: u32 = 0;
    let mut total: u32 = 0;
    for c in text.chars() {
        if c.is_whitespace() {
            continue;
        }
        total += 1;
        if is_text_like(c) {
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

fn needs_ocr(text: &str, ocr: &OcrConfig) -> bool {
    let non_ws = text.chars().filter(|c| !c.is_whitespace()).count();
    if non_ws < ocr.min_chars_for_check {
        return false;
    }
    decodability_score(text) < ocr.min_decodable_ratio
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
    suffix: Option<&str>,
) -> String {
    let mut scored: Vec<(usize, f32)> = bad_indices
        .iter()
        .map(|&i| (i, decodability_score(&pages[i])))
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
        let score = decodability_score(text);
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
        let score = decodability_score(text);
        assert!(
            score < 0.7,
            "garbled CID scored {score} (must be below 0.7 default threshold)"
        );
    }

    #[test]
    fn decodability_empty_text() {
        assert_eq!(decodability_score(""), 1.0);
    }

    #[test]
    fn decodability_table_of_numbers_high() {
        let text = "123 456\n789 012\n345 678\n";
        let score = decodability_score(text);
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
        };
        // Five non-whitespace chars — way under the threshold — but page is
        // too short to trust the score, so we don't flag it.
        assert!(!needs_ocr("ÊÌÃÕÜ", &cfg));
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
        };
        let text = "ÊivviVÌÛiÊÜ iÌ ÀÜ7 i>ÊÌ ÀÜÜi>«ÃÊÕÃi`Ê >Ì > VL>Ì]Ê \
                    Ì iÊ V >À>VÌiÀÊ ÕÃÌÊ ÕÃiÊ iÀÊ iiiÊ LÌÞ°Ê \
                    ÕÃ >Ê>ÀÌ>>ÀÌÃÊÜi>«Ã iÊ>ÞÊÃÕLÃÌÌÕÌiÊ iÀÊ>ÀÌ>ÊÀÌÃÊvÀ";
        assert!(needs_ocr(text, &cfg));
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
        };
        let text = "The quick brown fox jumps over the lazy dog. \
                    Pack my box with five dozen liquor jugs. \
                    She sells seashells by the seashore.";
        assert!(!needs_ocr(text, &cfg));
    }
}
