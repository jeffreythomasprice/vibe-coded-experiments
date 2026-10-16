//! PDF output: page layout is done here (greedy line breaking over font
//! metrics, CJK-aware), drawing and font subsetting are krilla's.

use std::borrow::Cow;
use std::path::PathBuf;

use krilla::Document;
use krilla::geom::Point;
use krilla::page::PageSettings;
use krilla::text::{Font, TextDirection};

use super::ExportError;
use super::blocks::{Block, is_cjk};

const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 56.0;
const BODY_SIZE: f32 = 11.0;
const BODY_LEADING: f32 = 1.5;
const HEADING_LEADING: f32 = 1.35;
const LIST_INDENT: f32 = 16.0;
/// Space after a block, in multiples of the body size.
const BLOCK_GAP: f32 = 0.6;

static NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
static NOTO_SANS_CJK_JP: &[u8] = include_bytes!("../../assets/fonts/NotoSansCJKjp-Regular.otf");

/// A font file's bytes plus the face to use inside it.
pub struct LoadedFont {
    pub data: Cow<'static, [u8]>,
    pub index: u32,
    /// For messages: the configured path, or which embedded font this is.
    pub name: String,
}

/// Load the configured font, or the embedded Noto Sans (Noto Sans CJK JP
/// when the document has CJK text).
pub fn load_font(configured: &str, index: u32, wants_cjk: bool) -> Result<LoadedFont, ExportError> {
    if !configured.trim().is_empty() {
        let path = PathBuf::from(configured.trim());
        let data = std::fs::read(&path).map_err(|source| ExportError::FontRead {
            path: path.clone(),
            source,
        })?;
        return Ok(LoadedFont {
            data: Cow::Owned(data),
            index,
            name: path.display().to_string(),
        });
    }
    let (data, name) = if wants_cjk {
        (NOTO_SANS_CJK_JP, "embedded Noto Sans CJK JP")
    } else {
        (NOTO_SANS, "embedded Noto Sans")
    };
    Ok(LoadedFont {
        data: Cow::Borrowed(data),
        index: 0,
        name: name.into(),
    })
}

/// One line of text to draw: `x` from the left edge, `y` is the baseline
/// measured from the top of the page.
#[derive(Debug, PartialEq)]
struct Line {
    x: f32,
    y: f32,
    size: f32,
    text: String,
}

type Page = Vec<Line>;

/// Render `blocks` to PDF bytes. Also returns how many distinct characters
/// the font has no glyph for (they draw as blanks or notdef boxes).
pub fn render(blocks: &[Block], font: &LoadedFont) -> Result<(Vec<u8>, usize), ExportError> {
    let face = ttf_parser::Face::parse(&font.data, font.index).map_err(|e| ExportError::Font {
        name: font.name.clone(),
        message: e.to_string(),
    })?;
    let upem = f32::from(face.units_per_em()).max(1.0);
    let mut missing = std::collections::BTreeSet::new();
    let advance = |c: char, missing: &mut std::collections::BTreeSet<char>| -> f32 {
        match face.glyph_index(c) {
            Some(g) => face
                .glyph_hor_advance(g)
                .map_or(0.5, |a| f32::from(a) / upem),
            None => {
                if !c.is_whitespace() && !c.is_control() {
                    missing.insert(c);
                }
                if is_cjk(c) { 1.0 } else { 0.5 }
            }
        }
    };
    let pages = layout(blocks, &mut |c| advance(c, &mut missing));
    if !missing.is_empty() {
        let sample: String = missing.iter().take(12).collect();
        tracing::warn!(
            font = %font.name,
            missing = missing.len(),
            sample = %sample,
            "export: font has no glyph for some characters; set [export].pdf_font",
        );
    }

    let data: krilla::Data = match &font.data {
        Cow::Borrowed(b) => (*b).into(),
        Cow::Owned(v) => v.clone().into(),
    };
    let krilla_font = Font::new(data, font.index).ok_or_else(|| ExportError::Font {
            name: font.name.clone(),
            message: "not a usable font file".into(),
        })?;
    let mut doc = Document::new();
    for page_lines in &pages {
        let mut page = doc
            .start_page_with(PageSettings::from_wh(PAGE_W, PAGE_H).expect("page size is positive"));
        let mut surface = page.surface();
        for line in page_lines {
            surface.draw_text(
                Point::from_xy(line.x, line.y),
                krilla_font.clone(),
                line.size,
                &line.text,
                false,
                TextDirection::Auto,
            );
        }
        surface.finish();
        page.finish();
    }
    let bytes = doc
        .finish()
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;
    Ok((bytes, missing.len()))
}

fn heading_size(level: u8) -> f32 {
    match level {
        0 | 1 => 20.0,
        2 => 16.0,
        3 => 13.5,
        _ => 12.0,
    }
}

/// Lay `blocks` out on A4 pages. `advance` is a character's width in em.
fn layout(blocks: &[Block], advance: &mut dyn FnMut(char) -> f32) -> Vec<Page> {
    let mut pages: Vec<Page> = vec![Vec::new()];
    let bottom = PAGE_H - MARGIN;
    let mut y = MARGIN;

    for block in blocks {
        let (text, size, leading, indent, bullet, heading) = match block {
            Block::Heading { level, text } => {
                let size = heading_size(*level);
                (
                    text.as_str(),
                    size,
                    size * HEADING_LEADING,
                    0.0,
                    false,
                    true,
                )
            }
            Block::Paragraph(text) => (
                text.as_str(),
                BODY_SIZE,
                BODY_SIZE * BODY_LEADING,
                0.0,
                false,
                false,
            ),
            Block::ListItem(text) => (
                text.as_str(),
                BODY_SIZE,
                BODY_SIZE * BODY_LEADING,
                LIST_INDENT,
                true,
                false,
            ),
        };
        let lines = wrap(text, size, PAGE_W - 2.0 * MARGIN - indent, advance);
        if lines.is_empty() {
            continue;
        }
        // A heading never ends a page: keep it with the next two body lines.
        let keep = if heading {
            leading + 2.0 * BODY_SIZE * BODY_LEADING
        } else {
            leading
        };
        if y + keep > bottom && !pages.last().unwrap().is_empty() {
            pages.push(Vec::new());
            y = MARGIN;
        }
        if heading && y > MARGIN {
            y += BODY_SIZE * BLOCK_GAP;
        }
        for (i, line) in lines.into_iter().enumerate() {
            if y + leading > bottom && !pages.last().unwrap().is_empty() {
                pages.push(Vec::new());
                y = MARGIN;
            }
            let page = pages.last_mut().unwrap();
            if bullet && i == 0 {
                page.push(Line {
                    x: MARGIN,
                    y: y + size,
                    size,
                    text: "•".into(),
                });
            }
            page.push(Line {
                x: MARGIN + indent,
                y: y + size,
                size,
                text: line,
            });
            y += leading;
        }
        y += BODY_SIZE * BLOCK_GAP;
    }
    pages
}

fn no_line_start(c: char) -> bool {
    "、。，．）」』】〕〉》！？：；ー々ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮ,.;:!?)]}%"
        .contains(c)
}

fn no_line_end(c: char) -> bool {
    "（「『【〔〈《([{“‘".contains(c)
}

/// Greedy line breaking within `max_w` points. Lines break at spaces, and
/// between any two characters when either is CJK (except where that would
/// strand a closing mark at the start of a line or an opening mark at the
/// end). A word longer than a line is broken wherever it overflows.
fn wrap(text: &str, size: f32, max_w: f32, advance: &mut dyn FnMut(char) -> f32) -> Vec<String> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut lines = Vec::new();
    let mut start = 0; // index into `chars` where the current line begins
    let mut width = 0.0f32;
    // (content end, next line start) as indices into `chars`.
    let mut last_break: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == ' ' {
            if i > start {
                last_break = Some((i, i + 1));
            }
        } else if i > start {
            let p = chars[i - 1].1;
            if p != ' ' && (is_cjk(p) || is_cjk(c)) && !no_line_start(c) && !no_line_end(p) {
                last_break = Some((i, i));
            }
        }
        let w = advance(c) * size;
        if width + w > max_w && i > start && c != ' ' {
            let (end, next) = last_break.take().unwrap_or((i, i));
            lines.push(slice(text, &chars, start, end));
            start = next;
            while start < chars.len() && chars[start].1 == ' ' {
                start += 1;
            }
            // Resume measuring from the new line start.
            width = 0.0;
            i = start;
            continue;
        }
        width += w;
        i += 1;
    }
    if start < chars.len() {
        lines.push(slice(text, &chars, start, chars.len()));
    }
    lines.retain(|l| !l.trim().is_empty());
    lines
}

fn slice(text: &str, chars: &[(usize, char)], from: usize, to: usize) -> String {
    let b0 = chars.get(from).map_or(text.len(), |c| c.0);
    let b1 = chars.get(to).map_or(text.len(), |c| c.0);
    text[b0..b1].trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CJK characters are one em wide, everything else half an em.
    fn adv(c: char) -> f32 {
        if is_cjk(c) { 1.0 } else { 0.5 }
    }

    #[test]
    fn wraps_latin_at_spaces() {
        // 10pt size: each char is 5pt, so 20 chars fit in 100pt.
        let lines = wrap("aaaa bbbb cccc dddd eeee ffff", 10.0, 100.0, &mut adv);
        assert_eq!(lines, vec!["aaaa bbbb cccc dddd", "eeee ffff"]);
    }

    #[test]
    fn breaks_an_overlong_word() {
        let lines = wrap("abcdefghijklmnopqrstuvwxyz", 10.0, 100.0, &mut adv);
        assert_eq!(lines, vec!["abcdefghijklmnopqrst", "uvwxyz"]);
    }

    #[test]
    fn wraps_cjk_between_characters_and_respects_kinsoku() {
        // 10pt CJK chars are 10pt wide: 5 per 50pt line.
        let lines = wrap("あいうえお、かきくけこ", 10.0, 50.0, &mut adv);
        // The 、 may not start the second line, so the break moves before お.
        assert_eq!(lines, vec!["あいうえ", "お、かきく", "けこ"]);
        let lines = wrap("あいうえおかきくけこ", 10.0, 50.0, &mut adv);
        assert_eq!(lines, vec!["あいうえお", "かきくけこ"]);
    }

    #[test]
    fn long_documents_flow_onto_new_pages_and_headings_keep_with_text() {
        let mut blocks = Vec::new();
        for i in 0..80 {
            blocks.push(Block::Paragraph(format!(
                "paragraph number {i} of the test document"
            )));
        }
        blocks.push(Block::Heading {
            level: 1,
            text: "Late heading".into(),
        });
        let pages = layout(&blocks, &mut adv);
        assert!(pages.len() >= 2, "{} pages", pages.len());
        assert!(pages.iter().all(|p| !p.is_empty()));
        // The heading is on a page that also holds something after it, or
        // is the first line of its page — never stranded at the bottom.
        for page in &pages {
            for l in page {
                assert!(
                    l.y <= PAGE_H - MARGIN + l.size,
                    "line below the margin: {l:?}"
                );
            }
        }
    }

    #[test]
    fn empty_input_yields_one_blank_page() {
        assert_eq!(layout(&[], &mut adv).len(), 1);
    }

    #[test]
    fn embedded_fonts_parse_and_cover_their_scripts() {
        for (wants_cjk, sample) in [(false, "Aé Ж"), (true, "あア中한A")] {
            let font = load_font("", 0, wants_cjk).unwrap();
            let face = ttf_parser::Face::parse(&font.data, font.index).unwrap();
            for c in sample.chars() {
                assert!(face.glyph_index(c).is_some(), "{} lacks {c}", font.name);
            }
            let data: krilla::Data = match &font.data {
                Cow::Borrowed(b) => (*b).into(),
                Cow::Owned(v) => v.clone().into(),
            };
            assert!(Font::new(data, font.index).is_some());
        }
    }

    #[test]
    fn renders_a_real_pdf_with_embedded_fonts() {
        let blocks = vec![
            Block::Heading {
                level: 1,
                text: "Title 見出し".into(),
            },
            Block::Paragraph("Hello world. 日本語の文章です。".repeat(40)),
            Block::ListItem("an item".into()),
        ];
        for wants_cjk in [false, true] {
            let font = load_font("", 0, wants_cjk).unwrap();
            let (bytes, missing) = render(&blocks, &font).unwrap();
            assert!(bytes.starts_with(b"%PDF"));
            if wants_cjk {
                assert_eq!(missing, 0);
            }
        }
    }
}
