//! Semantic chunker: splits a UTF-8 string into overlapping spans, preferring
//! structural break points over arbitrary byte cuts.
//!
//! Boundary tiers, most to least preferred:
//!   1. Page break — any byte offset listed in `page_break_positions`
//!      (typically the start byte of pages 2..N for a PDF).
//!   2. Markdown heading — start of a line whose first non-space char is `#`.
//!   3. Paragraph — position just after a `\n\n` pair.
//!   4. Sentence end — position just after `[.!?]` followed by space or
//!      newline, with a small abbreviation guard for `Mr.`, `e.g.`, etc.
//!
//! Each chunk targets `target_bytes` bytes but may grow up to ~1.5× looking
//! for a boundary. If no boundary is found in the search window, the chunk
//! falls back to a UTF-8-safe hard cut at that soft cap.
//!
//! Spans tile the input: every byte of `text` is contained in at least one
//! chunk, and consecutive chunks share roughly `overlap_bytes` bytes (the
//! overlap window's start snaps forward to the earliest boundary inside it
//! when one exists).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub char_start: usize,
    pub char_end: usize,
}

pub struct ChunkParams<'a> {
    pub target_bytes: usize,
    pub overlap_bytes: usize,
    /// Byte offsets in `text` where a "page" begins — treated as the highest-
    /// priority break point. Empty for plain text. For PDFs, callers
    /// typically pass the `byte_start` of pages 2..N (page 1 starts at byte
    /// 0, which is never a useful chunk break).
    pub page_break_positions: &'a [usize],
}

/// Splits `text` into overlapping spans. `on_progress(current, total)` fires
/// once after each span is produced; `total` is an estimate computed up front
/// from `text.len()` and the target stride, clamped so the displayed fraction
/// never exceeds 100%.
pub fn chunk_semantic<F>(
    text: &str,
    params: ChunkParams<'_>,
    mut on_progress: F,
) -> Vec<ChunkSpan>
where
    F: FnMut(usize, usize),
{
    assert!(params.target_bytes > 0, "target chunk size must be > 0");
    assert!(
        params.overlap_bytes < params.target_bytes,
        "overlap must be smaller than target chunk size"
    );

    if text.is_empty() {
        return Vec::new();
    }

    let total = text.len();
    let target = params.target_bytes;
    let overlap = params.overlap_bytes;
    let soft_cap = target + target / 2;
    let min_chunk = target / 2;
    let estimated_total = estimate_chunk_count(total, target, overlap);

    let mut out: Vec<ChunkSpan> = Vec::new();
    let mut cursor: usize = 0;

    loop {
        // Remaining text fits comfortably in a single chunk: emit it and
        // finish. Subsumes both the "tiny input" case and the natural last
        // chunk of a longer document.
        if total - cursor <= target {
            let byte_end = total;
            push_span(text, &mut out, cursor, byte_end);
            on_progress(out.len(), estimated_total.max(out.len()));
            break;
        }

        let window_start = cursor + min_chunk;
        let ideal_end = cursor + target;
        let hard_end = (cursor + soft_cap).min(total);

        let byte_end = find_break_in_range(
            text,
            params.page_break_positions,
            window_start,
            hard_end,
            ideal_end,
        )
        .unwrap_or_else(|| snap_back_to_boundary(text, hard_end));

        push_span(text, &mut out, cursor, byte_end);

        let current = out.len();
        let total_for_progress = estimated_total.max(current);
        tracing::trace!(
            chunk_index = current - 1,
            byte_start = cursor,
            byte_end,
            current,
            total = total_for_progress,
            "chunking: emitted chunk",
        );
        on_progress(current, total_for_progress);

        if byte_end >= total {
            break;
        }

        // Overlap: slide back by overlap_bytes, then advance to the earliest
        // tier-1..4 boundary inside [slide, byte_end). Falls back to the
        // existing UTF-8 forward snap if no boundary is in range.
        let slide = byte_end.saturating_sub(overlap).max(cursor + 1);
        let next_cursor = find_overlap_boundary(
            text,
            params.page_break_positions,
            slide,
            byte_end,
        )
        .unwrap_or_else(|| snap_forward_to_boundary(text, slide));
        cursor = next_cursor.max(cursor + 1);

        if cursor >= total {
            break;
        }
    }

    out
}

fn push_span(text: &str, out: &mut Vec<ChunkSpan>, byte_start: usize, byte_end: usize) {
    debug_assert!(text.is_char_boundary(byte_start));
    debug_assert!(text.is_char_boundary(byte_end));
    let char_start = match out.last() {
        Some(prev) => prev.char_start + text[prev.byte_start..byte_start].chars().count(),
        None => text[..byte_start].chars().count(),
    };
    let char_end = char_start + text[byte_start..byte_end].chars().count();
    out.push(ChunkSpan {
        byte_start,
        byte_end,
        char_start,
        char_end,
    });
}

fn estimate_chunk_count(total: usize, target: usize, overlap: usize) -> usize {
    if total == 0 {
        0
    } else if total <= target {
        1
    } else {
        let stride = target - overlap;
        1 + (total - target).div_ceil(stride.max(1))
    }
}

fn snap_back_to_boundary(text: &str, mut idx: usize) -> usize {
    let total = text.len();
    if idx > total {
        idx = total;
    }
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn snap_forward_to_boundary(text: &str, mut idx: usize) -> usize {
    let total = text.len();
    while idx < total && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// Candidates for each tier, all guaranteed in `[lo, hi]`.
struct Candidates {
    page: Vec<usize>,
    heading: Vec<usize>,
    paragraph: Vec<usize>,
    sentence: Vec<usize>,
}

fn collect_candidates(
    text: &str,
    page_breaks: &[usize],
    lo: usize,
    hi: usize,
) -> Candidates {
    let bytes = text.as_bytes();
    let total = text.len();
    let lo = lo.min(total);
    let hi = hi.min(total);

    let page: Vec<usize> = page_breaks
        .iter()
        .copied()
        .filter(|&p| p >= lo && p <= hi)
        .collect();

    let mut heading = Vec::new();
    let mut paragraph = Vec::new();
    let mut sentence = Vec::new();

    // Heading at byte 0 if the document begins with optional whitespace + '#'.
    if lo == 0 && starts_with_heading(bytes, 0) {
        heading.push(0);
    }

    // Sentence / paragraph / heading-after-newline candidates produced by
    // delimiter at index `i` typically land at `i + 1` (heading) or `i + 2`
    // (paragraph, sentence). To catch candidates landing at `lo`, start the
    // scan two bytes earlier.
    let scan_start = lo.saturating_sub(2);
    let mut i = scan_start;
    while i < hi && i < total {
        let b = bytes[i];

        if b == b'\n' {
            let h_pos = i + 1;
            if h_pos >= lo
                && h_pos <= hi
                && h_pos < total
                && starts_with_heading(bytes, h_pos)
            {
                heading.push(h_pos);
            }
            if i + 1 < total && bytes[i + 1] == b'\n' {
                let p_pos = i + 2;
                if p_pos >= lo && p_pos <= hi {
                    paragraph.push(p_pos);
                }
            }
        }

        if (b == b'.' || b == b'!' || b == b'?')
            && i + 1 < total
            && (bytes[i + 1] == b' ' || bytes[i + 1] == b'\n')
        {
            let abbreviation = b == b'.' && is_abbreviation_before(bytes, i);
            if !abbreviation {
                let s_pos = i + 2;
                if s_pos >= lo && s_pos <= hi {
                    sentence.push(s_pos);
                }
            }
        }

        i += 1;
    }

    Candidates {
        page,
        heading,
        paragraph,
        sentence,
    }
}

fn starts_with_heading(bytes: &[u8], pos: usize) -> bool {
    let mut j = pos;
    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    j < bytes.len() && bytes[j] == b'#'
}

/// True if the run of ASCII letters immediately before `dot_idx` looks like
/// an abbreviation: 1–2 letters. Catches `Mr.`, `Dr.`, `Ms.`, `St.`, `e.g.`,
/// `i.e.`, and initials like `J.` without trying to do real NLP. Tolerates
/// false positives like a sentence-ending `I.` being treated as part of an
/// abbreviation, since the next acceptable boundary is rarely far away.
fn is_abbreviation_before(bytes: &[u8], dot_idx: usize) -> bool {
    let mut k = dot_idx;
    while k > 0 && bytes[k - 1].is_ascii_alphabetic() {
        k -= 1;
    }
    matches!(dot_idx - k, 1 | 2)
}

fn find_break_in_range(
    text: &str,
    page_breaks: &[usize],
    lo: usize,
    hi: usize,
    ideal: usize,
) -> Option<usize> {
    if lo > hi {
        return None;
    }
    let c = collect_candidates(text, page_breaks, lo, hi);
    closest_to(&c.page, ideal)
        .or_else(|| closest_to(&c.heading, ideal))
        .or_else(|| closest_to(&c.paragraph, ideal))
        .or_else(|| closest_to(&c.sentence, ideal))
}

fn closest_to(candidates: &[usize], ideal: usize) -> Option<usize> {
    candidates
        .iter()
        .copied()
        .min_by_key(|&p| p.max(ideal) - p.min(ideal))
}

fn find_overlap_boundary(
    text: &str,
    page_breaks: &[usize],
    lo: usize,
    hi_exclusive: usize,
) -> Option<usize> {
    if hi_exclusive == 0 || lo >= hi_exclusive {
        return None;
    }
    let c = collect_candidates(text, page_breaks, lo, hi_exclusive - 1);
    c.page
        .iter()
        .chain(c.heading.iter())
        .chain(c.paragraph.iter())
        .chain(c.sentence.iter())
        .copied()
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk_default<F>(text: &str, target: usize, overlap: usize, on_progress: F) -> Vec<ChunkSpan>
    where
        F: FnMut(usize, usize),
    {
        chunk_semantic(
            text,
            ChunkParams {
                target_bytes: target,
                overlap_bytes: overlap,
                page_break_positions: &[],
            },
            on_progress,
        )
    }

    fn chunk_no_progress(text: &str, target: usize, overlap: usize) -> Vec<ChunkSpan> {
        chunk_default(text, target, overlap, |_, _| {})
    }

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunk_no_progress("", 10, 2).is_empty());
    }

    #[test]
    fn single_chunk_when_text_smaller_than_target() {
        let t = "hello world";
        let cs = chunk_no_progress(t, 100, 10);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].byte_start, 0);
        assert_eq!(cs[0].byte_end, t.len());
        assert_eq!(cs[0].char_start, 0);
        assert_eq!(cs[0].char_end, t.chars().count());
    }

    #[test]
    fn tiny_text_yields_single_chunk() {
        let t = "abc";
        let cs = chunk_no_progress(t, 5000, 1000);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].byte_end, t.len());
    }

    #[test]
    fn no_boundaries_degrades_to_byte_cap() {
        // 50 KB of 'a' has no paragraph / sentence / heading / page breaks.
        // Every chunk should be roughly the soft cap (1.5 × target) since
        // the search finds nothing in [min_chunk, soft_cap] and the fallback
        // cuts at hard_end.
        let target = 5000;
        let overlap = 1000;
        let soft_cap = target + target / 2;
        let t: String = std::iter::repeat('a').take(50_000).collect();
        let cs = chunk_no_progress(&t, target, overlap);
        // Final chunk may be smaller. All earlier chunks must hit the soft cap.
        for c in &cs[..cs.len() - 1] {
            assert_eq!(c.byte_end - c.byte_start, soft_cap, "chunk {:?}", c);
        }
        // All boundaries are valid UTF-8 boundaries (trivial here, but kept
        // for parity with the multibyte test below).
        for c in &cs {
            assert!(t.is_char_boundary(c.byte_start));
            assert!(t.is_char_boundary(c.byte_end));
        }
        assert_eq!(cs.last().unwrap().byte_end, t.len());
    }

    #[test]
    fn snaps_back_at_multibyte_boundary() {
        // 5 × 2-byte 'é' + 1 × 1-byte 'e' = 11 bytes.
        let t = "ééééée";
        assert_eq!(t.len(), 11);
        // No boundaries → falls back to hard cap snap. target=5, soft_cap=7.
        let cs = chunk_no_progress(t, 5, 1);
        for c in &cs {
            assert!(t.is_char_boundary(c.byte_start));
            assert!(t.is_char_boundary(c.byte_end));
            let slice = &t[c.byte_start..c.byte_end];
            assert_eq!(slice.chars().count(), c.char_end - c.char_start);
        }
        assert_eq!(cs.first().unwrap().byte_start, 0);
        assert_eq!(cs.last().unwrap().byte_end, t.len());
    }

    #[test]
    fn final_chunk_ends_at_text_end() {
        let t = "abcdefghijklmnop";
        let cs = chunk_no_progress(t, 4, 1);
        assert_eq!(cs.last().unwrap().byte_end, t.len());
    }

    #[test]
    fn chunks_cover_all_bytes() {
        // Mix of ASCII letters and punctuation so some sentence boundaries exist.
        let t: String = (0..200)
            .map(|i| if i % 30 == 29 { '.' } else if i % 30 == 28 { ' ' } else { 'x' })
            .collect();
        let cs = chunk_no_progress(&t, 60, 15);
        let mut covered = vec![false; t.len()];
        for c in &cs {
            for b in c.byte_start..c.byte_end {
                covered[b] = true;
            }
        }
        assert!(covered.iter().all(|x| *x));
    }

    #[test]
    fn paragraph_only_text_breaks_at_blank_lines() {
        // Six ~1100-byte paragraphs separated by \n\n. Target 1000 → each
        // break should land just after a \n\n.
        let para: String = std::iter::repeat('a').take(1100).collect();
        let t: String = std::iter::repeat(para.as_str())
            .take(6)
            .collect::<Vec<_>>()
            .join("\n\n");
        let cs = chunk_no_progress(&t, 1000, 100);
        // Every chunk after the first should start right after a \n\n
        // (or at byte 0 for the first). Every chunk except possibly the
        // last should end right after a \n\n inside the soft-cap window.
        for c in &cs[..cs.len() - 1] {
            assert!(c.byte_end >= 2);
            assert_eq!(
                &t.as_bytes()[c.byte_end - 2..c.byte_end],
                b"\n\n",
                "chunk ending {} did not end on \\n\\n",
                c.byte_end,
            );
        }
    }

    #[test]
    fn pdf_form_feed_preferred_over_paragraph() {
        // 1500 bytes of 'a', then \n\n (paragraph), then \u{000C} (page),
        // then more text. Target 1000, soft cap 1500. Both candidates fall
        // in the window — the page break should win.
        let prefix: String = std::iter::repeat('a').take(900).collect();
        let mid: String = std::iter::repeat('b').take(200).collect();
        // Layout: prefix (900) + "\n\n" (2) + mid (200) + "\u{000C}" (1) + tail.
        // Paragraph break candidate: position 902 (right after \n\n).
        // Page break candidate: position 1103 (right after form-feed).
        let tail: String = std::iter::repeat('c').take(2000).collect();
        let t = format!("{prefix}\n\n{mid}\u{000C}{tail}");
        let page_start_of_second = prefix.len() + 2 + mid.len() + '\u{000C}'.len_utf8();
        let page_breaks = vec![page_start_of_second];
        let cs = chunk_semantic(
            &t,
            ChunkParams {
                target_bytes: 1000,
                overlap_bytes: 200,
                page_break_positions: &page_breaks,
            },
            |_, _| {},
        );
        // First chunk should end right at the page break position.
        assert_eq!(cs[0].byte_end, page_start_of_second);
    }

    #[test]
    fn heading_preferred_over_sentence_and_paragraph() {
        // Place a sentence end and a paragraph break early in the window,
        // then a heading slightly later. Heading should win.
        let prefix: String = std::iter::repeat('a').take(550).collect();
        // 550 'a' + ". " (sentence at 552) + 'b' * 100 + "\n\n" (paragraph at 654) +
        // 'c' * 50 + "\n# Heading\n" + tail (heading at 706).
        let mid1: String = std::iter::repeat('b').take(100).collect();
        let mid2: String = std::iter::repeat('c').take(50).collect();
        let tail: String = std::iter::repeat('d').take(2000).collect();
        let t = format!("{prefix}. {mid1}\n\n{mid2}\n# Heading\n{tail}");
        // Position of heading-line start (the byte right after the '\n'
        // preceding '# Heading'):
        let heading_pos = prefix.len() + 2 + mid1.len() + 2 + mid2.len() + 1;
        assert_eq!(&t.as_bytes()[heading_pos..heading_pos + 1], b"#");
        let cs = chunk_no_progress(&t, 600, 100);
        // The first chunk should end at the heading line start because
        // heading beats paragraph beats sentence.
        assert_eq!(cs[0].byte_end, heading_pos);
    }

    #[test]
    fn abbreviations_not_split() {
        // "Mr. Smith said hello. Then he left." should break after "hello."
        // (skipping "Mr.") and definitely not after "Mr." itself.
        let prefix: String = std::iter::repeat('x').take(600).collect();
        let t = format!("{prefix}Mr. Smith said hello. Then he left.{}", "y".repeat(2000));
        // Sentence break = (index of period) + 2.
        let hello_break = prefix.len() + "Mr. Smith said hello".len() + 2;
        let mr_break = prefix.len() + "Mr".len() + 2;
        let cs = chunk_no_progress(&t, 650, 100);
        assert_eq!(cs[0].byte_end, hello_break);
        assert_ne!(cs[0].byte_end, mr_break);
    }

    #[test]
    fn overlap_lands_on_boundary_when_possible() {
        // Many 100-byte paragraphs separated by \n\n. With target=1000 and
        // overlap=500 the overlap window contains several paragraph
        // boundaries — chunk 2 should start at the earliest one (snap
        // forward as little as possible to preserve overlap).
        let para: String = std::iter::repeat('a').take(100).collect();
        let t: String = std::iter::repeat(para.as_str())
            .take(30)
            .collect::<Vec<_>>()
            .join("\n\n");
        let cs = chunk_no_progress(&t, 1000, 500);
        assert!(cs.len() >= 2);
        let bs = cs[1].byte_start;
        assert!(bs >= 2);
        assert_eq!(
            &t.as_bytes()[bs - 2..bs],
            b"\n\n",
            "chunk 2 should start right after a paragraph break, got byte_start={bs}",
        );
    }

    #[test]
    fn progress_callback_fires_once_per_chunk_with_monotonic_current() {
        // 50 KB of 'a' is enough to trigger several chunks at default-ish
        // sizing.
        let t: String = std::iter::repeat('a').take(50_000).collect();
        let mut events: Vec<(usize, usize)> = Vec::new();
        let cs = chunk_default(&t, 5000, 1000, |c, tot| events.push((c, tot)));
        assert_eq!(events.len(), cs.len());
        for (i, (current, total)) in events.iter().enumerate() {
            assert_eq!(*current, i + 1);
            assert!(*total >= *current);
        }
        assert_eq!(events.last().unwrap().0, cs.len());
    }

    #[test]
    fn forward_progress_when_overlap_is_large() {
        // Pathological-ish: overlap close to target. Cursor must still
        // advance every iteration.
        let t: String = std::iter::repeat("abcde fghij. ")
            .take(2000)
            .collect();
        let cs = chunk_no_progress(&t, 100, 99);
        let mut prev_start = 0usize;
        for c in cs.iter().skip(1) {
            assert!(c.byte_start > prev_start, "no forward progress");
            prev_start = c.byte_start;
        }
        assert_eq!(cs.last().unwrap().byte_end, t.len());
    }
}
