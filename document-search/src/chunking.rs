//! Pure chunker: splits a UTF-8 string into fixed-byte-size overlapping spans.
//!
//! Spans tile the input completely: every byte of `text` is contained in at
//! least one chunk, and consecutive chunks share `overlap_bytes` bytes
//! (snapped to the nearest UTF-8 char boundary). `byte_end` and `char_end`
//! are exclusive.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub char_start: usize,
    pub char_end: usize,
}

pub fn chunk(text: &str, size_bytes: usize, overlap_bytes: usize) -> Vec<ChunkSpan> {
    assert!(size_bytes > 0, "chunk size must be > 0");
    assert!(
        overlap_bytes < size_bytes,
        "overlap must be smaller than chunk size"
    );

    if text.is_empty() {
        return Vec::new();
    }

    let total = text.len();
    let mut out = Vec::new();
    let mut cursor: usize = 0;

    loop {
        let target = cursor.saturating_add(size_bytes).min(total);
        let byte_end = snap_back_to_boundary(text, target);

        let char_start = text[..cursor].chars().count();
        let char_end = char_start + text[cursor..byte_end].chars().count();
        out.push(ChunkSpan {
            byte_start: cursor,
            byte_end,
            char_start,
            char_end,
        });

        if byte_end >= total {
            break;
        }

        let next_target = byte_end.saturating_sub(overlap_bytes).max(cursor + 1);
        cursor = snap_forward_to_boundary(text, next_target);
        if cursor >= total {
            break;
        }
    }

    out
}

fn snap_back_to_boundary(text: &str, mut idx: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunk("", 10, 2).is_empty());
    }

    #[test]
    fn single_chunk_when_text_smaller_than_size() {
        let t = "hello world";
        let cs = chunk(t, 100, 10);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].byte_start, 0);
        assert_eq!(cs[0].byte_end, t.len());
        assert_eq!(cs[0].char_start, 0);
        assert_eq!(cs[0].char_end, t.chars().count());
    }

    #[test]
    fn ascii_chunks_tile_with_overlap() {
        let t: String = (b'a'..=b'z')
            .cycle()
            .take(25)
            .map(|c| c as char)
            .collect();
        let cs = chunk(&t, 10, 3);
        assert_eq!(cs[0].byte_start, 0);
        assert_eq!(cs[0].byte_end, 10);
        assert_eq!(cs[1].byte_start, 7);
        assert_eq!(cs[1].byte_end, 17);
        assert_eq!(cs[2].byte_start, 14);
        assert_eq!(cs[2].byte_end, 24);
        assert_eq!(cs[3].byte_start, 21);
        assert_eq!(cs[3].byte_end, 25);
        assert_eq!(cs.last().unwrap().byte_end, t.len());
        for c in &cs {
            assert_eq!(c.char_start, c.byte_start);
            assert_eq!(c.char_end, c.byte_end);
        }
    }

    #[test]
    fn snaps_back_at_multibyte_boundary() {
        // Five 2-byte "é"s plus one 1-byte "e" = 11 bytes total.
        let t = "ééééée";
        assert_eq!(t.len(), 11);
        // size 5 lands mid-codepoint; should snap back to byte 4.
        let cs = chunk(t, 5, 1);
        for c in &cs {
            assert!(t.is_char_boundary(c.byte_start));
            assert!(t.is_char_boundary(c.byte_end));
            // byte slice and the equivalent char slice must agree.
            let slice = &t[c.byte_start..c.byte_end];
            assert_eq!(slice.chars().count(), c.char_end - c.char_start);
        }
        assert_eq!(cs.first().unwrap().byte_start, 0);
        assert_eq!(cs.last().unwrap().byte_end, t.len());
    }

    #[test]
    fn final_chunk_ends_at_text_end() {
        let t = "abcdefghij";
        let cs = chunk(t, 4, 1);
        assert_eq!(cs.last().unwrap().byte_end, t.len());
    }

    #[test]
    fn chunks_cover_all_bytes() {
        let t: String = (0u32..200).map(|i| char::from_u32(i + 0x40).unwrap()).collect();
        let cs = chunk(&t, 17, 5);
        let mut covered = vec![false; t.len()];
        for c in &cs {
            for b in c.byte_start..c.byte_end {
                covered[b] = true;
            }
        }
        assert!(covered.iter().all(|x| *x));
    }
}
