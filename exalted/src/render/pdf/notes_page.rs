//! Append one or more "Notes" pages to the rendered character PDF.
//!
//! The MrGone character sheet template has no fields to hold user notes (or
//! XP history), so when the character carries any of those we append
//! generated plain-text pages to the end of the document. Layout uses the
//! built-in Type1 Helvetica font so no font embedding is required.

use lopdf::{Document, Object, Stream, dictionary};

use super::PdfRenderError;
use crate::character::{Character, Note, XpAward};
use crate::rules::database::database;

// US Letter MediaBox.
const PAGE_W: f64 = 612.0;
const PAGE_H: f64 = 792.0;
const MARGIN: f64 = 54.0;

const TITLE_SIZE: f64 = 18.0;
const HEADING_SIZE: f64 = 13.0;
const BODY_SIZE: f64 = 11.0;
const LEADING: f64 = 14.0;

// Conservative average glyph width estimate for Helvetica 11pt. Used to
// wrap body lines without pulling in real font metrics.
const CHARS_PER_LINE: usize = 88;

pub(super) fn append(doc: &mut Document, c: &Character) -> Result<(), PdfRenderError> {
    let sections = collect_sections(c);
    if sections.is_empty() {
        return Ok(());
    }

    let lines = build_lines(&sections);
    let pages = paginate(&lines);

    for page_lines in pages {
        append_page(doc, &page_lines)?;
    }
    Ok(())
}

#[derive(Debug)]
struct Section {
    heading: String,
    notes: Vec<NoteView>,
}

#[derive(Debug)]
struct NoteView {
    timestamp: String,
    body: String,
}

fn collect_sections(c: &Character) -> Vec<Section> {
    let db = database();
    let mut out: Vec<Section> = Vec::new();

    if !c.xp_awards.is_empty() {
        out.push(Section {
            heading: "Experience".to_string(),
            notes: c.xp_awards.iter().map(award_view).collect(),
        });
    }

    if !c.notes.is_empty() {
        out.push(Section {
            heading: "Character".to_string(),
            notes: c.notes.iter().map(note_view).collect(),
        });
    }
    for bg in &c.backgrounds {
        if bg.notes().is_empty() {
            continue;
        }
        let name = bg.display_name(db);
        let heading = if bg.label().is_empty() {
            format!("Background: {}", name)
        } else {
            format!("Background: {} ({})", name, bg.label())
        };
        out.push(Section {
            heading,
            notes: bg.notes().iter().map(note_view).collect(),
        });
    }
    for ch in &c.charms {
        if ch.notes().is_empty() {
            continue;
        }
        out.push(Section {
            heading: format!("Charm: {}", ch.display_name(db)),
            notes: ch.notes().iter().map(note_view).collect(),
        });
    }
    for sp in &c.spells {
        if sp.notes().is_empty() {
            continue;
        }
        out.push(Section {
            heading: format!("Spell: {}", sp.display_name(db)),
            notes: sp.notes().iter().map(note_view).collect(),
        });
    }
    for combo in &c.combos {
        if combo.notes.is_empty() {
            continue;
        }
        let name = if combo.name.is_empty() {
            "<unnamed>"
        } else {
            combo.name.as_str()
        };
        out.push(Section {
            heading: format!("Combo: {}", name),
            notes: combo.notes.iter().map(note_view).collect(),
        });
    }
    out
}

fn note_view(n: &Note) -> NoteView {
    let created = n.created_at.format("%Y-%m-%d %H:%M UTC");
    let timestamp = if n.created_at == n.updated_at {
        created.to_string()
    } else {
        format!(
            "{} (updated {})",
            created,
            n.updated_at.format("%Y-%m-%d %H:%M UTC")
        )
    };
    NoteView {
        timestamp,
        body: n.body.clone(),
    }
}

fn award_view(a: &XpAward) -> NoteView {
    let created = a.created_at.format("%Y-%m-%d %H:%M UTC");
    let timestamp = if a.created_at == a.updated_at {
        format!("{} — +{} XP", created, a.amount)
    } else {
        format!(
            "{} (updated {}) — +{} XP",
            created,
            a.updated_at.format("%Y-%m-%d %H:%M UTC"),
            a.amount,
        )
    };
    NoteView {
        timestamp,
        body: a.body.clone(),
    }
}

/// One logical line of output, tagged with its visual style.
#[derive(Debug, Clone)]
enum Line {
    /// Page title, drawn once at the top of the first page.
    Title,
    /// Blank vertical space.
    Blank,
    /// Section heading (bg/charm/spell/etc.).
    Heading(String),
    /// Note timestamp line.
    Timestamp(String),
    /// Wrapped body line (continuation lines are also Body).
    Body(String),
}

fn build_lines(sections: &[Section]) -> Vec<Line> {
    let mut out = vec![Line::Title, Line::Blank];
    for (i, sec) in sections.iter().enumerate() {
        if i > 0 {
            out.push(Line::Blank);
        }
        out.push(Line::Heading(sec.heading.clone()));
        for note in &sec.notes {
            out.push(Line::Timestamp(note.timestamp.clone()));
            for wrapped in wrap_body(&note.body) {
                out.push(Line::Body(wrapped));
            }
        }
    }
    out
}

fn wrap_body(body: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in body.split('\n') {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
                continue;
            }
            if current.chars().count() + 1 + word.chars().count() <= CHARS_PER_LINE {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Split into per-page line slices. Honors a fixed line budget per page.
fn paginate(lines: &[Line]) -> Vec<Vec<Line>> {
    // Body lines per page. Title line costs more vertical space but only
    // appears on page 1, so we budget the first page slightly tighter.
    const LINES_PER_FIRST_PAGE: usize = 44;
    const LINES_PER_PAGE: usize = 48;

    let mut pages: Vec<Vec<Line>> = Vec::new();
    let mut current: Vec<Line> = Vec::new();
    let mut budget = LINES_PER_FIRST_PAGE;
    for line in lines {
        if current.len() >= budget {
            pages.push(std::mem::take(&mut current));
            budget = LINES_PER_PAGE;
        }
        current.push(line.clone());
    }
    if !current.is_empty() {
        pages.push(current);
    }
    pages
}

fn append_page(doc: &mut Document, lines: &[Line]) -> Result<(), PdfRenderError> {
    let content = build_content_stream(lines);

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let font_bold_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
            "F2" => font_bold_id,
        },
    });
    let content_id = doc.add_object(Stream::new(lopdf::Dictionary::new(), content));

    // Reserve the page id so we can point /Parent at the existing Pages tree
    // and add the page into /Kids in one go.
    let pages_root_id = pages_root(doc)?;
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_root_id,
        "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
        "Resources" => resources_id,
        "Contents" => content_id,
    });

    append_to_pages_tree(doc, pages_root_id, page_id)
}

fn pages_root(doc: &Document) -> Result<lopdf::ObjectId, PdfRenderError> {
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(PdfRenderError::TemplateParse)?;
    let catalog = doc
        .get_dictionary(root_id)
        .map_err(PdfRenderError::TemplateParse)?;
    catalog
        .get(b"Pages")
        .and_then(|o| o.as_reference())
        .map_err(PdfRenderError::TemplateParse)
}

fn append_to_pages_tree(
    doc: &mut Document,
    pages_root_id: lopdf::ObjectId,
    new_page_id: lopdf::ObjectId,
) -> Result<(), PdfRenderError> {
    let pages = doc
        .get_object_mut(pages_root_id)
        .map_err(PdfRenderError::TemplateParse)?
        .as_dict_mut()
        .map_err(PdfRenderError::TemplateParse)?;

    let mut kids = pages
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .cloned()
        .unwrap_or_default();
    kids.push(Object::Reference(new_page_id));
    let new_count = kids.len() as i64;

    pages.set("Kids", Object::Array(kids));
    pages.set("Count", Object::Integer(new_count));
    Ok(())
}

fn build_content_stream(lines: &[Line]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut y = PAGE_H - MARGIN;

    // Background fill is just plain text — no graphics state needed.
    for line in lines {
        match line {
            Line::Title => {
                emit_text(&mut out, MARGIN, y, "F2", TITLE_SIZE, "Notes");
                y -= TITLE_SIZE + 6.0;
            }
            Line::Blank => {
                y -= LEADING;
            }
            Line::Heading(text) => {
                emit_text(&mut out, MARGIN, y, "F2", HEADING_SIZE, text);
                y -= LEADING;
            }
            Line::Timestamp(text) => {
                emit_text(
                    &mut out,
                    MARGIN + 12.0,
                    y,
                    "F1",
                    BODY_SIZE - 1.0,
                    &format!("[{}]", text),
                );
                y -= LEADING - 2.0;
            }
            Line::Body(text) => {
                emit_text(&mut out, MARGIN + 24.0, y, "F1", BODY_SIZE, text);
                y -= LEADING;
            }
        }
    }
    out
}

fn emit_text(out: &mut Vec<u8>, x: f64, y: f64, font: &str, size: f64, text: &str) {
    out.extend_from_slice(b"BT\n");
    out.extend_from_slice(format!("/{} {} Tf\n", font, size).as_bytes());
    out.extend_from_slice(format!("{:.2} {:.2} Td\n", x, y).as_bytes());
    out.extend_from_slice(b"(");
    out.extend_from_slice(&pdf_escape(text));
    out.extend_from_slice(b") Tj\n");
    out.extend_from_slice(b"ET\n");
}

/// Escape a string for a PDF literal (Tj operand), encoding it as WinAnsi.
/// Replaces common Unicode punctuation with their WinAnsi-mapped byte; falls
/// back to '?' for unrecognized non-ASCII characters.
fn pdf_escape(s: &str) -> Vec<u8> {
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
            // WinAnsi punctuation mappings for chars we use in notes.
            '\u{2013}' => out.push(0x96), // en dash
            '\u{2014}' => out.push(0x97), // em dash
            '\u{2018}' => out.push(0x91), // left single quote
            '\u{2019}' => out.push(0x92), // right single quote
            '\u{201C}' => out.push(0x93), // left double quote
            '\u{201D}' => out.push(0x94), // right double quote
            '\u{2026}' => out.push(0x85), // ellipsis
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
    fn wrap_body_breaks_at_word_boundaries() {
        // 20 ten-char words separated by spaces -> 220 chars across one line.
        // Must wrap into multiple lines, none exceeding CHARS_PER_LINE.
        let word = "abcdefghij";
        let body: Vec<&str> = (0..20).map(|_| word).collect();
        let body = body.join(" ");
        let wrapped = wrap_body(&body);
        assert!(wrapped.len() >= 2, "expected wrap, got {wrapped:?}");
        for line in &wrapped {
            assert!(
                line.chars().count() <= CHARS_PER_LINE,
                "line too long: {line:?}"
            );
        }
    }

    #[test]
    fn wrap_body_preserves_blank_lines() {
        let body = "first\n\nthird";
        let wrapped = wrap_body(body);
        assert_eq!(wrapped, vec!["first", "", "third"]);
    }

    #[test]
    fn pdf_escape_handles_parens_and_em_dash() {
        let bytes = pdf_escape("hi (there) — and");
        assert!(bytes.windows(2).any(|w| w == b"\\("));
        assert!(bytes.windows(2).any(|w| w == b"\\)"));
        assert!(bytes.contains(&0x97));
    }
}
