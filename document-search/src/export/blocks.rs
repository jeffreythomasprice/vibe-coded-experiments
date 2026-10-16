//! Heuristic document structure. The canonical text carries no markup, so
//! headings and list items are guessed from the shape of short lines. The
//! result feeds both the markdown and the PDF renderer, so they agree.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph(String),
    ListItem(String),
}

/// Split `text` into blocks. Form feeds (page breaks) and blank lines both
/// end a paragraph; lines inside a paragraph are re-joined.
pub fn parse_blocks(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\u{c}', "\n\n");
    let mut blocks = Vec::new();
    let mut group: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush_group(&mut group, &mut blocks);
        } else {
            group.push(line);
        }
    }
    flush_group(&mut group, &mut blocks);
    blocks
}

fn flush_group(group: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if group.is_empty() {
        return;
    }
    if group.len() == 1 {
        let line = group[0];
        if let Some(item) = strip_bullet(line) {
            blocks.push(Block::ListItem(item.to_string()));
        } else if let Some(level) = heading_level(line) {
            blocks.push(Block::Heading {
                level,
                text: line.to_string(),
            });
        } else {
            blocks.push(Block::Paragraph(line.to_string()));
        }
        group.clear();
        return;
    }

    // `true` while `cur` is a list item rather than a plain paragraph.
    let mut cur: Option<(bool, String)> = None;
    let flush_cur = |cur: &mut Option<(bool, String)>, blocks: &mut Vec<Block>| match cur.take() {
        Some((true, s)) => blocks.push(Block::ListItem(s)),
        Some((false, s)) => blocks.push(Block::Paragraph(s)),
        None => {}
    };
    for line in group.drain(..) {
        if let Some(item) = strip_bullet(line) {
            flush_cur(&mut cur, blocks);
            cur = Some((true, item.to_string()));
        } else if let Some((_, s)) = cur.as_mut() {
            join_line(s, line);
        } else {
            cur = Some((false, line.to_string()));
        }
    }
    flush_cur(&mut cur, blocks);
}

/// Append `next` to `acc`: no space between two CJK characters, a hyphen
/// broken across lines is rejoined, otherwise a single space.
fn join_line(acc: &mut String, next: &str) {
    let (Some(last), Some(first)) = (acc.chars().last(), next.chars().next()) else {
        acc.push_str(next);
        return;
    };
    if is_cjk(last) && is_cjk(first) {
        acc.push_str(next);
    } else if last == '-'
        && first.is_lowercase()
        && acc.chars().rev().nth(1).is_some_and(char::is_alphabetic)
    {
        acc.pop();
        acc.push_str(next);
    } else {
        acc.push(' ');
        acc.push_str(next);
    }
}

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x30FF   // CJK punctuation, hiragana, katakana
        | 0x3400..=0x4DBF // extension A
        | 0x4E00..=0x9FFF // unified ideographs
        | 0xAC00..=0xD7AF // hangul syllables
        | 0xF900..=0xFAFF // compatibility ideographs
        | 0xFF00..=0xFFEF // full-width forms
        | 0x20000..=0x2FFFF)
}

/// The text after a bullet marker, if `line` starts with one.
fn strip_bullet(line: &str) -> Option<&str> {
    let mut chars = line.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    match first {
        '•' | '・' | '●' | '▪' | '◦' | '‣' | '■' => {
            let rest = rest.trim_start();
            (!rest.is_empty()).then_some(rest)
        }
        '-' | '*' | '–' | '—' if rest.starts_with(' ') => {
            let rest = rest.trim_start();
            (!rest.is_empty()).then_some(rest)
        }
        _ => None,
    }
}

/// Markdown heading level (1–4) for a single-line paragraph that looks like
/// a heading, `None` otherwise.
pub(crate) fn heading_level(line: &str) -> Option<u8> {
    let line = line.trim();
    let n = line.chars().count();
    let cjk = line.chars().any(is_cjk);
    if n < 2 || n > if cjk { 40 } else { 80 } {
        return None;
    }
    if matches!(
        line.chars().last()?,
        '.' | '。' | ',' | '、' | ';' | '；' | ':' | '：'
    ) {
        return None;
    }

    let lower = line.to_lowercase();
    for (kw, level) in [("chapter", 1), ("part", 1), ("section", 2)] {
        if let Some(rest) = lower.strip_prefix(kw)
            && rest.starts_with(' ')
            && rest
                .trim_start()
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
        {
            return Some(level);
        }
    }
    if let Some(level) = cjk_numbered(line) {
        return Some(level);
    }
    if let Some(level) = decimal_numbered(line) {
        return Some(level);
    }

    let letters: Vec<char> = line.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase()) {
        return Some(2);
    }
    None
}

/// `第一章 …` (chapter/part, level 1) and `第二節 …` (section, level 2).
fn cjk_numbered(line: &str) -> Option<u8> {
    let rest = line.strip_prefix('第')?;
    let mut count = 0;
    for c in rest.chars() {
        match c {
            '章' | '部' | '編' | '编' if count > 0 => return Some(1),
            '節' | '节' if count > 0 => return Some(2),
            '0'..='9'
            | '０'..='９'
            | '〇'
            | '一'
            | '二'
            | '三'
            | '四'
            | '五'
            | '六'
            | '七'
            | '八'
            | '九'
            | '十'
            | '百'
            | '千' => count += 1,
            _ => return None,
        }
        if count > 6 {
            return None;
        }
    }
    None
}

/// `1 Title`, `1. Title`, `2.3 Title`, `2.3.1 Title`.
fn decimal_numbered(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut groups = 0;
    loop {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let digits = i - start;
        if digits == 0 || digits > 3 {
            return None;
        }
        groups += 1;
        // Another group only if the dot is followed by a digit.
        if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
            i += 1;
        } else {
            break;
        }
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
    }
    let title = &line[i..];
    if !title.starts_with(char::is_whitespace) {
        return None;
    }
    let first = title.trim_start().chars().next()?;
    if !first.is_alphabetic() || first.is_lowercase() {
        return None;
    }
    Some(match groups {
        1 => 2,
        2 => 3,
        _ => 4,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Block {
        Block::Paragraph(s.into())
    }
    fn h(level: u8, s: &str) -> Block {
        Block::Heading {
            level,
            text: s.into(),
        }
    }

    #[test]
    fn wrapped_lines_join_into_one_paragraph() {
        let blocks = parse_blocks("The quick brown\nfox jumps over\n\nSecond one.");
        assert_eq!(
            blocks,
            vec![p("The quick brown fox jumps over"), p("Second one.")]
        );
    }

    #[test]
    fn form_feed_ends_a_paragraph() {
        let blocks = parse_blocks("end of page one\u{c}start of page two");
        assert_eq!(blocks, vec![p("end of page one"), p("start of page two")]);
    }

    #[test]
    fn cjk_lines_join_without_a_space_and_hyphens_rejoin() {
        assert_eq!(
            parse_blocks("日本語の\n文章です。"),
            vec![p("日本語の文章です。")]
        );
        assert_eq!(
            parse_blocks("an inter-\nnational deal"),
            vec![p("an international deal")]
        );
        assert_eq!(parse_blocks("well-\nKnown"), vec![p("well- Known")]);
    }

    #[test]
    fn headings_are_recognised_by_pattern_and_level() {
        assert_eq!(heading_level("Chapter 3: The Return"), Some(1));
        assert_eq!(heading_level("PART II"), Some(1));
        assert_eq!(heading_level("Section 4"), Some(2));
        assert_eq!(heading_level("1. Introduction"), Some(2));
        assert_eq!(heading_level("2.3 Method details"), Some(3));
        assert_eq!(heading_level("2.3.1 Sampling"), Some(4));
        assert_eq!(heading_level("TABLE OF CONTENTS"), Some(2));
        assert_eq!(heading_level("第一章 はじめに"), Some(1));
        assert_eq!(heading_level("第二節 背景"), Some(2));
    }

    #[test]
    fn ordinary_lines_are_not_headings() {
        assert_eq!(heading_level("This ends like a sentence."), None);
        assert_eq!(heading_level("1. do the thing first, then more."), None);
        assert_eq!(heading_level("2020 results"), None);
        assert_eq!(heading_level("3 apples"), None);
        assert_eq!(heading_level("a"), None);
        assert_eq!(heading_level(&"X".repeat(100)), None);
        assert_eq!(heading_level("第一に、これは文です"), None);
    }

    #[test]
    fn multi_line_paragraphs_are_never_headings() {
        let blocks = parse_blocks("INTRODUCTION\nmore text follows");
        assert_eq!(blocks, vec![p("INTRODUCTION more text follows")]);
        assert_eq!(parse_blocks("INTRODUCTION"), vec![h(2, "INTRODUCTION")]);
    }

    #[test]
    fn bullets_become_list_items() {
        let blocks = parse_blocks("• one\n• two\n  continued\n- three");
        assert_eq!(
            blocks,
            vec![
                Block::ListItem("one".into()),
                Block::ListItem("two continued".into()),
                Block::ListItem("three".into()),
            ]
        );
        assert_eq!(parse_blocks("・単独"), vec![Block::ListItem("単独".into())]);
        assert_eq!(parse_blocks("-5 degrees"), vec![p("-5 degrees")]);
    }
}
