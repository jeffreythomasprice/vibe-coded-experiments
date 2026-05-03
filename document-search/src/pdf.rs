//! PDF text extraction. Wraps `pdf_extract::extract_text_by_pages` and
//! produces a single canonical text string with per-page byte/char offsets.
//! Pages are joined by a form-feed (`\u{000C}`) separator that becomes part
//! of the canonical text — chunk indices and `total_size_bytes` are computed
//! against this same string.

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

pub fn extract(bytes: &[u8]) -> Result<PdfText, IngestError> {
    let pages =
        pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|e| IngestError::PdfExtract {
            source: Box::new(e),
        })?;

    let mut full_text = String::new();
    let mut page_offsets = Vec::with_capacity(pages.len());
    let mut char_cursor: usize = 0;

    for (i, page) in pages.iter().enumerate() {
        if i > 0 {
            full_text.push_str(PAGE_SEPARATOR);
            char_cursor += PAGE_SEPARATOR.chars().count();
        }
        let byte_start = full_text.len();
        let char_start = char_cursor;
        full_text.push_str(page);
        let byte_end = full_text.len();
        char_cursor += page.chars().count();
        let char_end = char_cursor;

        page_offsets.push(PageOffset {
            page_number: (i + 1) as u32,
            byte_start,
            byte_end,
            char_start,
            char_end,
        });
    }

    Ok(PdfText {
        full_text,
        page_offsets,
    })
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
