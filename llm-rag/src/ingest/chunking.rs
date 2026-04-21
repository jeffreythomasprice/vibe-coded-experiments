//! Text chunking for document ingest.
//!
//! `chunk` slides a char window across the input (not a byte window — a
//! byte-sized window could split a multi-byte UTF-8 codepoint). The window is
//! sized in chars for predictability across scripts; byte offsets into the
//! original string are tracked separately and stored on each `Chunk` so the
//! DAL can persist `ChunkRange::Bytes` ranges back into the document.

/// Chunk width in Unicode scalar values (chars).
pub const CHUNK_CHARS: usize = 1000;

/// Overlap between consecutive chunks, also in chars. Must be strictly less
/// than `CHUNK_CHARS` so the window advances each step.
pub const OVERLAP_CHARS: usize = 100;

const _: () = assert!(
    OVERLAP_CHARS < CHUNK_CHARS,
    "OVERLAP_CHARS must be strictly less than CHUNK_CHARS"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub content: String,
    /// Byte offset (inclusive) into the source string.
    pub byte_start: u64,
    /// Byte offset (exclusive) into the source string.
    pub byte_end: u64,
}

/// Split `text` into overlapping chunks. Returns `vec![]` for empty input.
/// For input shorter than `CHUNK_CHARS` chars, returns a single chunk spanning
/// the full input.
pub fn chunk(text: &str) -> Vec<Chunk> {
    if text.is_empty() {
        return Vec::new();
    }

    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let total_chars = offsets.len();
    let total_bytes = text.len();

    if total_chars <= CHUNK_CHARS {
        return vec![Chunk {
            content: text.to_string(),
            byte_start: 0,
            byte_end: total_bytes as u64,
        }];
    }

    let step = CHUNK_CHARS - OVERLAP_CHARS;
    let mut out = Vec::new();
    let mut start_char = 0usize;
    loop {
        let end_char = (start_char + CHUNK_CHARS).min(total_chars);
        let byte_start = offsets[start_char];
        let byte_end = if end_char == total_chars {
            total_bytes
        } else {
            offsets[end_char]
        };
        out.push(Chunk {
            content: text[byte_start..byte_end].to_string(),
            byte_start: byte_start as u64,
            byte_end: byte_end as u64,
        });
        if end_char == total_chars {
            break;
        }
        start_char += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_no_chunks() {
        assert!(chunk("").is_empty());
    }

    #[test]
    fn short_input_single_chunk_spans_full_bytes() {
        let s = "hello world";
        let chunks = chunk(s);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, s);
        assert_eq!(chunks[0].byte_start, 0);
        assert_eq!(chunks[0].byte_end, s.len() as u64);
    }

    #[test]
    fn consecutive_chunks_overlap_by_constant() {
        // Make sure we exceed CHUNK_CHARS so we get more than one chunk.
        let text: String = (0..CHUNK_CHARS * 3)
            .map(|i| (b'a' + (i % 26) as u8) as char)
            .collect();
        let chunks = chunk(&text);
        assert!(chunks.len() >= 3);
        for pair in chunks.windows(2) {
            // Each chunk should contain CHUNK_CHARS chars (except possibly the
            // last, which is <= CHUNK_CHARS).
            let a = pair[0].content.chars().count();
            let b = pair[1].content.chars().count();
            assert_eq!(a, CHUNK_CHARS);
            assert!(b <= CHUNK_CHARS);
            // The overlap in chars is OVERLAP_CHARS: the last OVERLAP_CHARS of
            // `a` should equal the first OVERLAP_CHARS of `b`.
            let a_tail: String = pair[0].content.chars().rev().take(OVERLAP_CHARS).collect();
            let a_tail: String = a_tail.chars().rev().collect();
            let b_head: String = pair[1].content.chars().take(OVERLAP_CHARS).collect();
            assert_eq!(a_tail, b_head);
        }
    }

    #[test]
    fn utf8_boundary_safety_multibyte_codepoints() {
        // Japanese + emoji — chars are 3 and 4 bytes respectively.
        let unit = "あいうえおかきくけこ🌸🌱🌷";
        let text: String = unit.repeat(400);
        let chunks = chunk(&text);
        assert!(chunks.len() > 1);
        // Every chunk's byte range must land on UTF-8 boundaries — round-trip
        // the slice through the original string to verify.
        for c in &chunks {
            let bytes = &text.as_bytes()[c.byte_start as usize..c.byte_end as usize];
            let decoded =
                std::str::from_utf8(bytes).expect("chunk byte range not on UTF-8 boundary");
            assert_eq!(decoded, c.content);
        }
    }

    #[test]
    fn byte_offsets_cover_full_input() {
        let text: String = (0..CHUNK_CHARS * 2 + 50).map(|_| 'x').collect();
        let chunks = chunk(&text);
        assert_eq!(chunks.first().unwrap().byte_start, 0);
        assert_eq!(chunks.last().unwrap().byte_end, text.len() as u64);
    }
}
