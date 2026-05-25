//! Text-encoding helpers for PDF output.
//!
//! Two different PDF contexts need different encodings:
//!
//! - **Text strings** (e.g., an AcroForm field's `/V` value): per the PDF spec
//!   these are interpreted as PDFDocEncoding unless they start with the
//!   UTF-16BE BOM `FE FF`. We always emit UTF-16BE so any Unicode survives.
//!
//! - **Content-stream literal strings** (operands to `Tj`/`TJ`): these are
//!   decoded through the operand font's encoding. The MrGone template's
//!   `/Helv` resource (and our notes-page Helvetica) use WinAnsiEncoding, so
//!   we transcode UTF-8 → WinAnsi bytes for those.

/// Encode `s` as the bytes of a PDF text string. Always UTF-16BE prefixed
/// with the `FE FF` BOM, so every Unicode code point round-trips through PDF
/// viewers' text-string decoding (which would otherwise misinterpret raw
/// UTF-8 bytes as PDFDocEncoding — turning `—` into `â•ﬂ`, etc.).
///
/// Wrap the returned bytes in `Object::String(_, StringFormat::Hexadecimal)`
/// to avoid surprises from `\` / paren bytes inside the UTF-16 stream.
pub(super) fn to_text_string(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + s.len() * 2);
    out.push(0xFE);
    out.push(0xFF);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

/// Transcode `s` to bytes for a content-stream literal `( … )` operand,
/// assuming the active font uses WinAnsiEncoding. Escapes `\`, `(`, `)` and
/// control chars; maps the Unicode punctuation we actually emit (dashes,
/// curly quotes, ellipsis, bullet, …). Anything else outside Latin-1 falls
/// back to `?`.
pub(super) fn winansi_literal(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.extend_from_slice(b"\\\\"),
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            c if (c as u32) < 0x80 => out.push(c as u8),
            '\u{2013}' => out.push(0x96), // en dash
            '\u{2014}' => out.push(0x97), // em dash
            '\u{2018}' => out.push(0x91), // left single quote
            '\u{2019}' => out.push(0x92), // right single quote
            '\u{201A}' => out.push(0x82), // single low-9 quote
            '\u{201C}' => out.push(0x93), // left double quote
            '\u{201D}' => out.push(0x94), // right double quote
            '\u{201E}' => out.push(0x84), // double low-9 quote
            '\u{2020}' => out.push(0x86), // dagger
            '\u{2021}' => out.push(0x87), // double dagger
            '\u{2022}' => out.push(0x95), // bullet
            '\u{2026}' => out.push(0x85), // ellipsis
            '\u{2030}' => out.push(0x89), // per mille
            '\u{2039}' => out.push(0x8B), // single guillemet left
            '\u{203A}' => out.push(0x9B), // single guillemet right
            '\u{2122}' => out.push(0x99), // trademark
            '\u{00A0}'..='\u{00FF}' => out.push(ch as u8),
            _ => out.push(b'?'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_string_prepends_utf16_bom() {
        let bytes = to_text_string("A");
        assert_eq!(bytes, vec![0xFE, 0xFF, 0x00, 0x41]);
    }

    #[test]
    fn text_string_round_trips_em_dash() {
        // U+2014 EM DASH -> UTF-16BE 0x2014
        let bytes = to_text_string("—");
        assert_eq!(bytes, vec![0xFE, 0xFF, 0x20, 0x14]);
    }

    #[test]
    fn text_string_handles_supplementary_plane() {
        // U+1F600 = surrogate pair D83D DE00
        let bytes = to_text_string("\u{1F600}");
        assert_eq!(bytes, vec![0xFE, 0xFF, 0xD8, 0x3D, 0xDE, 0x00]);
    }

    #[test]
    fn winansi_maps_dashes() {
        assert_eq!(winansi_literal("–"), vec![0x96]);
        assert_eq!(winansi_literal("—"), vec![0x97]);
    }

    #[test]
    fn winansi_escapes_parens_and_backslash() {
        assert_eq!(winansi_literal("a(b)\\c"), b"a\\(b\\)\\\\c");
    }

    #[test]
    fn winansi_unknown_falls_back_to_question_mark() {
        assert_eq!(winansi_literal("\u{1F600}"), b"?");
    }
}
