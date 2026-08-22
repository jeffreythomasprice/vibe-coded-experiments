//! Incremental Server-Sent Events (SSE) frame decoding.
//!
//! Pure and transport-agnostic: feed it byte chunks as they arrive over the
//! wire, in any split (including mid-frame, or mid-UTF8-character — the
//! frame boundary is always an ASCII blank line, so splitting on it never
//! lands inside a multi-byte character), and it yields complete frames.
//! Both Anthropic's and OpenAI's streaming APIs use this framing; only the
//! `data:` JSON payload's shape differs, which each provider's `wire` module
//! interprets via `parse_stream_frame`.

/// One decoded SSE frame.
///
/// `event` defaults to `"message"` per the SSE spec when the frame carries no
/// `event:` line. `data` is every `data:` line for the frame joined with
/// `\n`, per the spec's rule for multi-line data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub event: String,
    pub data: String,
}

/// Buffers bytes across chunk boundaries and yields complete frames.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of bytes and return every complete frame it produced.
    /// Any trailing partial frame is held until the next call.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some((end, sep_len)) = find_frame_end(&self.buf) {
            let frame_bytes: Vec<u8> = self.buf.drain(..end + sep_len).collect();
            let text = String::from_utf8_lossy(&frame_bytes[..end]);
            if let Some(frame) = parse_frame(&text) {
                frames.push(frame);
            }
        }
        frames
    }
}

/// Find the earliest blank-line separator (`"\n\n"` or `"\r\n\r\n"`),
/// returning `(byte offset where the frame's own text ends, separator
/// length)`.
fn find_frame_end(buf: &[u8]) -> Option<(usize, usize)> {
    let lf_lf = find_subslice(buf, b"\n\n").map(|i| (i, 2));
    let crlf_crlf = find_subslice(buf, b"\r\n\r\n").map(|i| (i, 4));
    match (lf_lf, crlf_crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_frame(text: &str) -> Option<SseFrame> {
    let mut event = String::from("message");
    let mut data_lines = Vec::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim_start().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
        // Lines starting with `:` are comments/keep-alives; `id:`/`retry:`
        // are ignored too — none of our providers send them.
    }
    if data_lines.is_empty() {
        // A frame with no `data:` line at all — a comment-only keep-alive
        // like `":\n\n"` — carries nothing worth surfacing.
        return None;
    }
    Some(SseFrame {
        event,
        data: data_lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frames_on_blank_lines() {
        let mut decoder = SseDecoder::new();
        let frames = decoder.push(b"event: message_start\ndata: {\"a\":1}\n\nevent: ping\ndata: {}\n\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].event, "message_start");
        assert_eq!(frames[0].data, "{\"a\":1}");
        assert_eq!(frames[1].event, "ping");
        assert_eq!(frames[1].data, "{}");
    }

    #[test]
    fn defaults_event_to_message_when_absent() {
        let mut decoder = SseDecoder::new();
        let frames = decoder.push(b"data: {\"x\":1}\n\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "message");
    }

    #[test]
    fn joins_multi_line_data_fields() {
        let mut decoder = SseDecoder::new();
        let frames = decoder.push(b"data: line one\ndata: line two\n\n");
        assert_eq!(frames[0].data, "line one\nline two");
    }

    #[test]
    fn ignores_comment_only_frames() {
        let mut decoder = SseDecoder::new();
        let frames = decoder.push(b": keep-alive\n\ndata: real\n\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "real");
    }

    #[test]
    fn holds_a_partial_frame_until_the_terminator_arrives() {
        let mut decoder = SseDecoder::new();
        assert_eq!(decoder.push(b"event: content_block_delta\n"), vec![]);
        assert_eq!(decoder.push(b"data: {\"tex"), vec![]);
        let frames = decoder.push(b"t\":\"hi\"}\n\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "{\"text\":\"hi\"}");
    }

    #[test]
    fn handles_crlf_line_endings() {
        let mut decoder = SseDecoder::new();
        let frames = decoder.push(b"event: ping\r\ndata: {}\r\n\r\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "ping");
    }

    #[test]
    fn a_frame_split_mid_data_across_many_small_chunks_still_parses() {
        let mut decoder = SseDecoder::new();
        let whole = b"event: message_delta\ndata: {\"stop_reason\":\"end_turn\"}\n\n";
        let mut frames = Vec::new();
        for byte in whole {
            frames.extend(decoder.push(std::slice::from_ref(byte)));
        }
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "message_delta");
    }
}
