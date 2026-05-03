//! PDF text extraction. Shells out to `pdftotext` (poppler-utils) to produce
//! a single canonical text string with per-page byte/char offsets. pdftotext
//! emits each page followed by a U+000C form-feed; we split on it, then re-
//! join with the same separator so pages are separated *by*, not terminated
//! *with*, the form-feed. Chunk indices and `total_size_bytes` are computed
//! against this canonical string.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

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

/// Extract canonical PDF text by shelling out to `pdftotext`. We pass the
/// file path directly (rather than streaming bytes via stdin) — pdftotext on
/// `-` input is prone to deadlocks on large files, and we already have the
/// file on disk anyway. `on_page` fires once per page after extraction
/// completes; pdftotext doesn't surface mid-parse progress, so the callback
/// is best-effort to keep the existing progress-event shape.
pub fn extract<F>(path: &Path, mut on_page: F) -> Result<PdfText, IngestError>
where
    F: FnMut(u32, u32),
{
    let started = Instant::now();
    tracing::info!(path = %path.display(), "pdf: extracting via pdftotext");

    let output = Command::new("pdftotext")
        .arg("-q")
        .arg("-enc")
        .arg("UTF-8")
        .arg(path)
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(spawn_err)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("exited with status {}", output.status)
        } else {
            stderr
        };
        return Err(IngestError::PdfExtract {
            source: format!("pdftotext: {detail}").into(),
        });
    }

    // pdftotext emits each page followed by a form-feed. Strip the trailing
    // one so a 3-page doc becomes "p1<FF>p2<FF>p3" rather than "...p3<FF>",
    // matching the canonical-text invariant the rest of the codebase relies
    // on (offsets line up with the same string we hand to chunking).
    let raw = String::from_utf8_lossy(&output.stdout).into_owned();
    let pages: Vec<&str> = if raw.is_empty() {
        Vec::new()
    } else {
        raw.strip_suffix(PAGE_SEPARATOR)
            .unwrap_or(&raw)
            .split(PAGE_SEPARATOR)
            .collect()
    };

    let total_pages = pages.len() as u32;
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
        on_page(page_num, total_pages);
    }

    tracing::info!(
        total_pages,
        total_bytes = full_text.len(),
        total_secs = started.elapsed().as_secs_f64(),
        "pdf: extraction complete",
    );

    Ok(PdfText {
        full_text,
        page_offsets,
    })
}

fn spawn_err(e: std::io::Error) -> IngestError {
    let msg = if e.kind() == std::io::ErrorKind::NotFound {
        "pdftotext binary not found in PATH (install poppler-utils)".to_string()
    } else {
        format!("spawning pdftotext: {e}")
    };
    IngestError::PdfExtract { source: msg.into() }
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
