//! Text chunking for document ingest.
//!
//! `chunk` slides a char window across the input (not a byte window — a
//! byte-sized window could split a multi-byte UTF-8 codepoint). The window is
//! sized in chars for predictability across scripts; byte offsets into the
//! original string are tracked separately and stored on each `Chunk` so the
//! DAL can persist `ChunkRange::Bytes` ranges back into the document.
//!
//! Chunk size is picked per-ingest from the embedding model's advertised
//! context length — see [`chunk_sizes_for`]. Keeping the math off to the side
//! means callers decide once and hand us plain `usize`s.

/// Floor-clamp on any computed chunk width. Prevents pathological tiny
/// context lengths from producing degenerate 10-char chunks.
const MIN_CHUNK_CHARS: usize = 256;

/// Chars per token heuristic for English. Conservative: real ratios are
/// typically 3.5–4.0, so multiplying the model's token budget by this number
/// leaves room to spare for code or non-Latin scripts (which tokenize shorter
/// per char).
const CHARS_PER_TOKEN: f64 = 3.5;

/// Fraction of the model's context we actually fill — leave headroom for
/// tokenizer variance and any special tokens the provider prepends.
const CONTEXT_UTILIZATION: f64 = 0.9;

/// Overlap is a quarter of the chunk window — enough to keep a sentence or
/// paragraph boundary continuous across neighbours without bloating the row
/// count too much.
const OVERLAP_FRACTION: usize = 4;

/// Fallback chunk width when the provider didn't report a context length.
/// Sized to fit a 512-token model comfortably.
const DEFAULT_CHUNK_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub content: String,
    /// Byte offset (inclusive) into the source string.
    pub byte_start: u64,
    /// Byte offset (exclusive) into the source string.
    pub byte_end: u64,
}

/// Compute `(chunk_chars, overlap_chars)` for an embedding model whose
/// advertised context is `max_tokens`. `None` yields a conservative default.
pub fn chunk_sizes_for(max_tokens: Option<usize>) -> (usize, usize) {
    let chunk_chars = match max_tokens {
        Some(tokens) => {
            let raw = (tokens as f64 * CHARS_PER_TOKEN * CONTEXT_UTILIZATION).floor() as usize;
            raw.max(MIN_CHUNK_CHARS)
        }
        None => DEFAULT_CHUNK_CHARS,
    };
    let overlap_chars = (chunk_chars / OVERLAP_FRACTION).min(chunk_chars.saturating_sub(1));
    (chunk_chars, overlap_chars)
}

/// Split `text` into overlapping chunks of `chunk_chars` chars with
/// `overlap_chars` chars of overlap. Returns `vec![]` for empty input. For
/// input shorter than `chunk_chars` chars, returns a single chunk spanning
/// the full input.
///
/// `overlap_chars` must be strictly less than `chunk_chars` so the window
/// advances each step.
pub fn chunk(text: &str, chunk_chars: usize, overlap_chars: usize) -> Vec<Chunk> {
    debug_assert!(
        overlap_chars < chunk_chars,
        "overlap_chars must be strictly less than chunk_chars"
    );
    debug_assert!(chunk_chars > 0, "chunk_chars must be non-zero");

    if text.is_empty() {
        return Vec::new();
    }

    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let total_chars = offsets.len();
    let total_bytes = text.len();

    if total_chars <= chunk_chars {
        return vec![Chunk {
            content: text.to_string(),
            byte_start: 0,
            byte_end: total_bytes as u64,
        }];
    }

    let step = chunk_chars - overlap_chars;
    let mut out = Vec::new();
    let mut start_char = 0usize;
    loop {
        let end_char = (start_char + chunk_chars).min(total_chars);
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

    const TEST_CHUNK: usize = 1000;
    const TEST_OVERLAP: usize = 100;

    #[test]
    fn empty_returns_no_chunks() {
        assert!(chunk("", TEST_CHUNK, TEST_OVERLAP).is_empty());
    }

    #[test]
    fn short_input_single_chunk_spans_full_bytes() {
        let s = "hello world";
        let chunks = chunk(s, TEST_CHUNK, TEST_OVERLAP);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, s);
        assert_eq!(chunks[0].byte_start, 0);
        assert_eq!(chunks[0].byte_end, s.len() as u64);
    }

    #[test]
    fn consecutive_chunks_overlap_by_constant() {
        let text: String = (0..TEST_CHUNK * 3)
            .map(|i| (b'a' + (i % 26) as u8) as char)
            .collect();
        let chunks = chunk(&text, TEST_CHUNK, TEST_OVERLAP);
        assert!(chunks.len() >= 3);
        for pair in chunks.windows(2) {
            // Each chunk should contain TEST_CHUNK chars (except possibly the
            // last, which is <= TEST_CHUNK).
            let a = pair[0].content.chars().count();
            let b = pair[1].content.chars().count();
            assert_eq!(a, TEST_CHUNK);
            assert!(b <= TEST_CHUNK);
            // The overlap in chars is TEST_OVERLAP: the last TEST_OVERLAP of
            // `a` should equal the first TEST_OVERLAP of `b`.
            let a_tail: String = pair[0].content.chars().rev().take(TEST_OVERLAP).collect();
            let a_tail: String = a_tail.chars().rev().collect();
            let b_head: String = pair[1].content.chars().take(TEST_OVERLAP).collect();
            assert_eq!(a_tail, b_head);
        }
    }

    #[test]
    fn utf8_boundary_safety_multibyte_codepoints() {
        // Japanese + emoji — chars are 3 and 4 bytes respectively.
        let unit = "あいうえおかきくけこ🌸🌱🌷";
        let text: String = unit.repeat(400);
        let chunks = chunk(&text, TEST_CHUNK, TEST_OVERLAP);
        assert!(chunks.len() > 1);
        for c in &chunks {
            let bytes = &text.as_bytes()[c.byte_start as usize..c.byte_end as usize];
            let decoded =
                std::str::from_utf8(bytes).expect("chunk byte range not on UTF-8 boundary");
            assert_eq!(decoded, c.content);
        }
    }

    #[test]
    fn byte_offsets_cover_full_input() {
        let text: String = (0..TEST_CHUNK * 2 + 50).map(|_| 'x').collect();
        let chunks = chunk(&text, TEST_CHUNK, TEST_OVERLAP);
        assert_eq!(chunks.first().unwrap().byte_start, 0);
        assert_eq!(chunks.last().unwrap().byte_end, text.len() as u64);
    }

    #[test]
    fn chunk_sizes_for_none_is_default() {
        assert_eq!(
            chunk_sizes_for(None),
            (DEFAULT_CHUNK_CHARS, DEFAULT_CHUNK_CHARS / OVERLAP_FRACTION)
        );
    }

    #[test]
    fn chunk_sizes_for_2048_tokens_scales_and_keeps_overlap_quarter() {
        let (chunk_chars, overlap_chars) = chunk_sizes_for(Some(2048));
        // 2048 * 3.5 * 0.9 = 6451.2 -> floor 6451
        assert_eq!(chunk_chars, 6451);
        assert_eq!(overlap_chars, 6451 / 4);
        assert!(overlap_chars < chunk_chars);
    }

    #[test]
    fn chunk_sizes_for_tiny_model_clamped() {
        let (chunk_chars, overlap_chars) = chunk_sizes_for(Some(64));
        // 64 * 3.5 * 0.9 = 201.6 -> floor 201 -> clamp to 256
        assert_eq!(chunk_chars, MIN_CHUNK_CHARS);
        assert_eq!(overlap_chars, MIN_CHUNK_CHARS / 4);
        assert!(overlap_chars < chunk_chars);
    }
}
