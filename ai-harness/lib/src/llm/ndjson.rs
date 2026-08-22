//! Incremental newline-delimited JSON (NDJSON) line decoding.
//!
//! Ollama's streaming responses are NDJSON: one JSON object per line, with no
//! blank-line framing. Pure and transport-agnostic, like `sse`.

/// Buffers bytes across chunk boundaries and yields complete lines.
#[derive(Debug, Default)]
pub struct NdjsonDecoder {
    buf: Vec<u8>,
}

impl NdjsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of bytes and return every complete line it produced
    /// (trailing newline stripped, blank lines skipped). A trailing partial
    /// line is held until the next call.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buf.drain(..=pos).collect();
            let raw = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1]).into_owned();
            let line = raw.strip_suffix('\r').unwrap_or(&raw).to_string();
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_one_object_per_line() {
        let mut decoder = NdjsonDecoder::new();
        let lines = decoder.push(b"{\"a\":1}\n{\"b\":2}\n");
        assert_eq!(lines, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
    }

    #[test]
    fn holds_a_partial_line_across_chunk_boundaries() {
        let mut decoder = NdjsonDecoder::new();
        assert_eq!(decoder.push(b"{\"done\":fal"), Vec::<String>::new());
        let lines = decoder.push(b"se}\n");
        assert_eq!(lines, vec!["{\"done\":false}".to_string()]);
    }

    #[test]
    fn skips_blank_lines() {
        let mut decoder = NdjsonDecoder::new();
        let lines = decoder.push(b"{\"a\":1}\n\n{\"b\":2}\n");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn handles_crlf_line_endings() {
        let mut decoder = NdjsonDecoder::new();
        let lines = decoder.push(b"{\"a\":1}\r\n");
        assert_eq!(lines, vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn a_line_split_byte_by_byte_still_parses() {
        let mut decoder = NdjsonDecoder::new();
        let whole = b"{\"model\":\"llama3.1\"}\n";
        let mut lines = Vec::new();
        for byte in whole {
            lines.extend(decoder.push(std::slice::from_ref(byte)));
        }
        assert_eq!(lines, vec!["{\"model\":\"llama3.1\"}".to_string()]);
    }
}
