//! Merging two OCR readings of one page (NDLOCR-Lite = A, YomiToku = B).
//!
//! The two engines fail differently (A occasionally drops or repeats a line;
//! B misreads dashes and lets furigana through), so where they agree the text
//! is almost always right. `align` diffs them character by character,
//! ignoring whitespace, and groups the differences into hunks.
//!
//! `rule_choice` settles the hunks whose answer is known from the benchmark:
//! typographic-only differences (`fold`), B's misread or missing dashes, and
//! furigana B leaked. The rest, mostly one-character misreads, go to a vision
//! model that sees the page image and picks A or B per hunk. It never sees,
//! and so can't rewrite, the text both engines agreed on. The model is only
//! trusted when it gives the same answer with the readings in either order
//! (see `ocr_ja::arbitrate`): replayed on the benchmark it leaned towards
//! whichever reading came second, and given descriptions of each engine's
//! failure modes it followed the descriptions rather than the image.
//!
//! Line structure comes from B, which emits one line per paragraph; A emits
//! one line per printed column.

use similar::{Algorithm, DiffOp};
use unicode_normalization::UnicodeNormalization as _;

/// Equal runs this short between two hunks are folded into one hunk, so a
/// misread word reads as one spot rather than several one-char spots.
const MERGE_GAP: usize = 2;

/// Agreed characters shown on each side of a hunk.
const CONTEXT_CHARS: usize = 12;

/// Longest B-only kana run treated as leaked furigana. A's real drops are
/// whole lines; furigana glosses are a word's reading.
const MAX_FURIGANA_CHARS: usize = 8;

/// What B prints where A has a dash, when B misread or dropped it.
const DASH_MISREADS: &[char] = &['ー', '!', '！', '|', '｜', '、', '。'];

/// A closing bracket right after a dash that A drops along with the dash's
/// end of the column, and B keeps (`――」` reads as A `--`, B `ー」`).
const DASH_CLOSER: char = '」';

/// The dash as Japanese books print it. NDLOCR-Lite writes it as `-`/`--`.
const BOOK_DASH: &str = "――";

#[derive(Debug, Clone, Copy)]
struct Sym {
    c: char,
    /// A line break preceded this char in the engine's output.
    break_before: bool,
}

fn symbols(s: &str) -> Vec<Sym> {
    let mut out = Vec::new();
    let mut pending_break = false;
    for c in s.chars() {
        if c == '\n' {
            pending_break = !out.is_empty();
        } else if !c.is_whitespace() {
            out.push(Sym {
                c,
                break_before: std::mem::take(&mut pending_break),
            });
        }
    }
    out
}

fn text_of(syms: &[Sym]) -> String {
    syms.iter().map(|s| s.c).collect()
}

#[derive(Debug, Clone)]
enum Segment {
    Same(Vec<Sym>),
    Diff { a: Vec<Sym>, b: Vec<Sym> },
}

/// Which reading a hunk resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    A,
    B,
    /// Replacement text from a rule (A's dash, in book form).
    Text(String),
}

/// A disagreement as shown to the arbiter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub before: String,
    pub a: String,
    pub b: String,
    pub after: String,
}

impl Hunk {
    /// Fallback when the arbiter is unsure or unusable: for text only B has,
    /// keep it (furigana leaks were already settled by `rule_choice`, so this
    /// is most likely a line A dropped); otherwise trust A, the more accurate
    /// engine.
    pub fn default_choice(&self) -> Choice {
        if self.a.is_empty() {
            Choice::B
        } else {
            Choice::A
        }
    }
}

#[derive(Debug, Clone)]
pub struct Alignment {
    segments: Vec<Segment>,
    /// Indices into `segments` of the `Diff`s that need the arbiter, in order.
    open: Vec<usize>,
    /// `Diff`s settled by `rule_choice`, with the side chosen.
    auto: Vec<(usize, Choice)>,
}

/// Diff A against B and group the differences into hunks.
pub fn align(a: &str, b: &str) -> Alignment {
    let a_syms = symbols(a);
    let b_syms = symbols(b);
    let a_chars: Vec<char> = a_syms.iter().map(|s| s.c).collect();
    let b_chars: Vec<char> = b_syms.iter().map(|s| s.c).collect();
    let ops = similar::capture_diff_slices(Algorithm::Myers, &a_chars, &b_chars);

    // Raw runs as (is_equal, a range, b range).
    let mut runs: Vec<(bool, std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
    for op in ops {
        let (eq, ar, br) = match op {
            DiffOp::Equal { old_index, new_index, len } => {
                (true, old_index..old_index + len, new_index..new_index + len)
            }
            DiffOp::Delete { old_index, old_len, new_index } => {
                (false, old_index..old_index + old_len, new_index..new_index)
            }
            DiffOp::Insert { old_index, new_index, new_len } => {
                (false, old_index..old_index, new_index..new_index + new_len)
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => (
                false,
                old_index..old_index + old_len,
                new_index..new_index + new_len,
            ),
        };
        match runs.last_mut() {
            Some((last_eq, la, lb)) if *last_eq == eq => {
                la.end = ar.end;
                lb.end = br.end;
            }
            _ => runs.push((eq, ar, br)),
        }
    }

    // Fold short equal runs that sit between two differences into one hunk.
    let mut merged: Vec<(bool, std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
    let mut i = 0;
    while i < runs.len() {
        let (eq, ar, br) = runs[i].clone();
        let bridges = eq
            && ar.len() <= MERGE_GAP
            && matches!(merged.last(), Some((false, _, _)))
            && matches!(runs.get(i + 1), Some((false, _, _)));
        if (!eq || bridges)
            && let Some((false, la, lb)) = merged.last_mut()
        {
            la.end = ar.end;
            lb.end = br.end;
            i += 1;
            continue;
        }
        merged.push((eq, ar, br));
        i += 1;
    }

    let mut segments = Vec::with_capacity(merged.len());
    let mut open = Vec::new();
    let mut auto = Vec::new();
    for (eq, ar, br) in merged {
        if eq {
            // Keep B's line breaks for agreed text.
            segments.push(Segment::Same(b_syms[br].to_vec()));
            continue;
        }
        let a = a_syms[ar].to_vec();
        let b = b_syms[br].to_vec();
        let idx = segments.len();
        match rule_choice(&text_of(&a), &text_of(&b)) {
            Some(choice) => auto.push((idx, choice)),
            None => open.push(idx),
        }
        segments.push(Segment::Diff { a, b });
    }
    Alignment {
        segments,
        open,
        auto,
    }
}

/// Fold away purely typographic differences: character width and other
/// compatibility forms (NFKC), dash glyph and length, quote style, ellipsis
/// glyph and length. The long-vowel mark ー is *not* a dash: confusing the
/// two changes the text.
pub fn fold(s: &str) -> String {
    const DASHES: &[char] = &['―', '—', '–', '‐', '‑', '‒', '−', '－', '─', '━', '-'];
    let nfkc: String = s.nfkc().filter(|c| !c.is_whitespace()).collect();
    let mut out = String::with_capacity(nfkc.len());
    let chars: Vec<char> = nfkc.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if DASHES.contains(&c) {
            while i < chars.len() && DASHES.contains(&chars[i]) {
                i += 1;
            }
            out.push('―');
            continue;
        }
        if matches!(c, '…' | '‥') || chars[i..].starts_with(&['.', '.', '.']) || chars[i..].starts_with(&['・', '・', '・']) {
            while i < chars.len() {
                if matches!(chars[i], '…' | '‥') {
                    i += 1;
                } else if chars[i..].starts_with(&['.', '.', '.']) || chars[i..].starts_with(&['・', '・', '・']) {
                    i += 3;
                } else {
                    break;
                }
            }
            out.push('…');
            continue;
        }
        out.push(match c {
            '〝' | '〟' | '“' | '”' | '″' => '"',
            other => other,
        });
        i += 1;
    }
    out
}

/// Settle a hunk without the arbiter where the benchmark says what's right:
/// - readings that differ only typographically: the one with fewer ASCII
///   chars (books use full-width forms), B on a tie, with A's ASCII dashes
///   in book form;
/// - a dash in A against ー, !, |, a stray comma, or nothing in B: B misread
///   or dropped the dash, never the other way round. A `」` ending B's
///   reading is real (A drops it), so it's kept after the dash;
/// - short kana only B has: furigana B leaked into the text.
fn rule_choice(a: &str, b: &str) -> Option<Choice> {
    let a_dash = !a.is_empty() && fold(a).chars().all(|c| c == '―');
    let book_dash = || {
        if a.chars().all(|c| c == '-') {
            BOOK_DASH.to_string()
        } else {
            a.to_string()
        }
    };
    if fold(a) == fold(b) {
        let ascii = |s: &str| s.chars().filter(char::is_ascii).count();
        return Some(match (ascii(a) < ascii(b), a_dash) {
            (true, true) => Choice::Text(book_dash()),
            (true, false) => Choice::A,
            (false, _) => Choice::B,
        });
    }
    if a_dash {
        let (misread, closer) = match b.strip_suffix(DASH_CLOSER) {
            Some(rest) => (rest, true),
            None => (b, false),
        };
        if misread.chars().all(|c| DASH_MISREADS.contains(&c)) {
            let mut text = book_dash();
            if closer {
                text.push(DASH_CLOSER);
            }
            return Some(Choice::Text(text));
        }
    }
    let n = b.chars().count();
    if a.is_empty() && (1..=MAX_FURIGANA_CHARS).contains(&n) && b.chars().all(is_kana) {
        return Some(Choice::A);
    }
    None
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}')
}

impl Alignment {
    /// The hunks the arbiter must decide, in page order.
    pub fn hunks(&self) -> Vec<Hunk> {
        self.open
            .iter()
            .map(|&idx| {
                let Segment::Diff { a, b } = &self.segments[idx] else {
                    unreachable!("open indexes Diff segments");
                };
                Hunk {
                    before: self.context(idx, true),
                    a: text_of(a),
                    b: text_of(b),
                    after: self.context(idx, false),
                }
            })
            .collect()
    }

    /// Hunks settled by `fold`.
    pub fn auto_resolved(&self) -> usize {
        self.auto.len()
    }

    /// Up to `CONTEXT_CHARS` of text before (or after) segment `idx`, taking
    /// B's reading for neighbouring hunks.
    fn context(&self, idx: usize, before: bool) -> String {
        let mut chars: Vec<char> = Vec::new();
        let neighbours: Box<dyn Iterator<Item = &Segment>> = if before {
            Box::new(self.segments[..idx].iter().rev())
        } else {
            Box::new(self.segments[idx + 1..].iter())
        };
        for seg in neighbours {
            let syms = match seg {
                Segment::Same(s) => s,
                Segment::Diff { b, .. } => b,
            };
            if before {
                chars.extend(syms.iter().rev().map(|s| s.c));
            } else {
                chars.extend(syms.iter().map(|s| s.c));
            }
            if chars.len() >= CONTEXT_CHARS {
                break;
            }
        }
        chars.truncate(CONTEXT_CHARS);
        if before {
            chars.reverse();
        }
        chars.into_iter().collect()
    }

    /// The merged page text. `choices` answers `hunks()` in order.
    pub fn render(&self, choices: &[Choice]) -> String {
        assert_eq!(choices.len(), self.open.len(), "one choice per open hunk");
        let mut resolved: Vec<Option<&Choice>> = vec![None; self.segments.len()];
        for (&idx, c) in self.open.iter().zip(choices) {
            resolved[idx] = Some(c);
        }
        for (idx, c) in &self.auto {
            resolved[*idx] = Some(c);
        }

        let mut out = String::new();
        let mut push = |syms: &[Sym], first_break: bool| {
            for (i, s) in syms.iter().enumerate() {
                if (s.break_before || (i == 0 && first_break)) && !out.is_empty() {
                    out.push('\n');
                }
                out.push(s.c);
            }
        };
        for (idx, seg) in self.segments.iter().enumerate() {
            match seg {
                Segment::Same(syms) => push(syms, false),
                Segment::Diff { a, b } => {
                    let b_break = b.first().is_some_and(|s| s.break_before);
                    match resolved[idx].expect("every Diff is open or auto") {
                        Choice::B => push(b, false),
                        // A's breaks are column breaks, not paragraphs; only
                        // keep a paragraph break B had at this spot.
                        Choice::A => push(&strip_breaks(a), b_break),
                        Choice::Text(text) => push(&strip_breaks(&symbols(text)), b_break),
                    }
                }
            }
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

fn strip_breaks(syms: &[Sym]) -> Vec<Sym> {
    syms.iter()
        .map(|s| Sym {
            c: s.c,
            break_before: false,
        })
        .collect()
}

/// System prompt for the arbitration call. Deliberately says nothing about
/// either engine's habits: given those, the model followed them instead of
/// looking at the image.
pub const ARBITER_SYSTEM_PROMPT: &str = "\
You are proofreading OCR of one printed Japanese book page, shown in the image. \
Two OCR engines read the page and disagree at the numbered spots you are given. \
For each spot, look closely at the printed characters in the image at that spot \
and decide which reading matches the print exactly. The surrounding text \
(\"before\" and \"after\") is what both engines agreed on, and helps you find the \
spot. Both readings come from OCR, so either may be wrong; neither is more \
trustworthy in general.

Answer \"A\" or \"B\" for each spot.";

/// JSON schema the arbiter's reply must follow.
pub fn answer_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "answers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "integer"},
                        "choice": {"type": "string", "enum": ["A", "B"]}
                    },
                    "required": ["id", "choice"]
                }
            }
        },
        "required": ["answers"]
    })
}

/// User turn listing the hunks, numbered from 1. With `swapped`, each hunk's
/// readings are presented the other way round (A's text labelled "B"), for
/// the order-consistency check.
pub fn arbiter_user_message(hunks: &[Hunk], swapped: bool) -> String {
    let mut s = String::from(
        "Spots where the engines disagree. \"before\" and \"after\" are the agreed text \
         around each spot.\n\n",
    );
    for (i, h) in hunks.iter().enumerate() {
        let (first, second) = if swapped { (&h.b, &h.a) } else { (&h.a, &h.b) };
        let line = serde_json::json!({
            "id": i + 1,
            "before": h.before,
            "A": first,
            "B": second,
            "after": h.after,
        });
        s.push_str(&line.to_string());
        s.push('\n');
    }
    s.push_str(
        "\nReply with JSON only, one answer per spot, as \
         {\"answers\": [{\"id\": 1, \"choice\": \"A\"}]}.",
    );
    let n = hunks.len();
    s.push_str(&if n == 1 {
        " There is exactly 1 spot, id 1.".to_string()
    } else {
        format!(" There are exactly {n} spots, ids 1 to {n}.")
    });
    s
}

#[derive(serde::Deserialize)]
struct Reply {
    answers: Vec<Answer>,
}

#[derive(serde::Deserialize)]
struct Answer {
    id: usize,
    choice: String,
}

/// Parse and check the arbiter's reply to `arbiter_user_message(hunks,
/// swapped)`: valid JSON (a ```json fence is tolerated) with every id
/// answered exactly once. Choices come back in terms of the real A and B,
/// undoing `swapped`.
pub fn parse_answers(reply: &str, hunks: &[Hunk], swapped: bool) -> Result<Vec<Choice>, String> {
    let body = reply.trim();
    let body = body
        .strip_prefix("```json")
        .or_else(|| body.strip_prefix("```"))
        .map(|s| s.trim_end().trim_end_matches("```"))
        .unwrap_or(body);
    let parsed: Reply = serde_json::from_str(body).map_err(|e| format!("not valid JSON: {e}"))?;
    let mut choices: Vec<Option<Choice>> = vec![None; hunks.len()];
    for ans in parsed.answers {
        let slot = ans
            .id
            .checked_sub(1)
            .and_then(|i| choices.get_mut(i))
            .ok_or_else(|| format!("answer for unknown spot {}", ans.id))?;
        if slot.is_some() {
            return Err(format!("spot {} answered twice", ans.id));
        }
        let first = match ans.choice.as_str() {
            "A" | "a" => true,
            "B" | "b" => false,
            other => return Err(format!("spot {}: unknown choice {other:?}", ans.id)),
        };
        *slot = Some(if first != swapped { Choice::A } else { Choice::B });
    }
    choices
        .into_iter()
        .enumerate()
        .map(|(i, c)| c.ok_or_else(|| format!("spot {} not answered", i + 1)))
        .collect()
}

/// Combine the answers from both presentation orders: where they agree, take
/// the answer; where they don't, the model is guessing, so take the default.
/// Returns the choices and how many spots disagreed.
pub fn reconcile(hunks: &[Hunk], straight: &[Choice], swapped: &[Choice]) -> (Vec<Choice>, usize) {
    let mut unsure = 0;
    let choices = hunks
        .iter()
        .zip(straight.iter().zip(swapped))
        .map(|(h, (x, y))| {
            if x == y {
                x.clone()
            } else {
                unsure += 1;
                h.default_choice()
            }
        })
        .collect();
    (choices, unsure)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(a: &str, b: &str, pick: impl Fn(&Hunk) -> Choice) -> (Vec<Hunk>, String) {
        let al = align(a, b);
        let hunks = al.hunks();
        let choices: Vec<Choice> = hunks.iter().map(pick).collect();
        let text = al.render(&choices);
        (hunks, text)
    }

    #[test]
    fn agreement_needs_no_arbiter_and_keeps_b_paragraphs() {
        // A breaks per column, B per paragraph.
        let a = "雨が降って\nいた。彼は歩いた。\n次の朝。\n";
        let b = "雨が降っていた。彼は歩いた。\n次の朝。\n";
        let (hunks, text) = resolve(a, b, |_| unreachable!());
        assert!(hunks.is_empty());
        assert_eq!(text, "雨が降っていた。彼は歩いた。\n次の朝。\n");
    }

    #[test]
    fn dropped_line_is_one_hunk_and_default_keeps_it() {
        // v1p097: NDLOCR dropped 「ぼくたちの仲間が高熱病で倒れた。
        let b = "向き直った。「ぼくたちの仲間が高熱病で倒れた。それを治すためには";
        let a = "向き直った。それを治すためには";
        let (hunks, text) = resolve(a, b, Hunk::default_choice);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].a, "");
        assert_eq!(hunks[0].b, "「ぼくたちの仲間が高熱病で倒れた。");
        assert_eq!(hunks[0].before, "向き直った。");
        assert_eq!(text, format!("{b}\n"));
    }

    #[test]
    fn furigana_leak_is_dropped_by_rule() {
        // v1p200: YomiToku leaked the ruby メテオ.
        let a = "脳裏をよぎる。しかし、";
        let b = "脳裏をよぎる。メテオしかし、";
        let (hunks, text) = resolve(a, b, |_| unreachable!());
        assert!(hunks.is_empty());
        assert_eq!(text, "脳裏をよぎる。しかし、\n");
    }

    #[test]
    fn b_only_text_with_kanji_is_not_furigana() {
        let a = "向き直った。それを";
        let b = "向き直った。仲間がそれを";
        assert_eq!(align(a, b).hunks().len(), 1);
    }

    #[test]
    fn ascii_dashes_auto_resolve_to_full_width() {
        let a = "「テラ--!」どこからか";
        let b = "「テラ――!」どこからか";
        let al = align(a, b);
        assert!(al.hunks().is_empty());
        assert_eq!(al.auto_resolved(), 1);
        assert_eq!(al.render(&[]), "「テラ――!」どこからか\n");
    }

    #[test]
    fn misread_or_missing_dash_resolves_to_book_dash() {
        // NDLOCR's `-` against YomiToku's ー, !, or nothing.
        for b in ["「ぼくはー」ギルバート", "「ぼくは!」ギルバート", "「ぼくは」ギルバート", "「ぼくは、」ギルバート"] {
            let (hunks, text) = resolve("「ぼくは-」ギルバート", b, |_| unreachable!());
            assert!(hunks.is_empty(), "{b}");
            assert_eq!(text, "「ぼくは――」ギルバート\n", "{b}");
        }
    }

    #[test]
    fn closing_bracket_after_misread_dash_is_kept() {
        // v2p026: printed 「磁力か――」; NDLOCR `--` (bracket lost), YomiToku ー」.
        let (hunks, text) = resolve("「磁力か--ダーク", "「磁力かー」ダーク", |_| unreachable!());
        assert!(hunks.is_empty());
        assert_eq!(text, "「磁力か――」ダーク\n");
    }

    #[test]
    fn substitutions_go_to_arbiter() {
        let (hunks, text) = resolve("爪弾こうとした竪琴を", "爪弾こうとした堅琴を", |_| Choice::A);
        assert_eq!(hunks.len(), 1);
        assert_eq!(text, "爪弾こうとした竪琴を\n");
    }

    #[test]
    fn nearby_differences_merge_into_one_hunk() {
        // Two misread chars one apart read as one spot.
        let a = "歓喜の表情";
        let b = "勧喜表情";
        let al = align(a, b);
        assert_eq!(al.hunks().len(), 1);
    }

    #[test]
    fn choosing_a_keeps_b_paragraph_break() {
        let a = "終わった。始まりだ。";
        let b = "終わった。\n姶まりだ。";
        let (_, text) = resolve(a, b, |_| Choice::A);
        assert_eq!(text, "終わった。\n始まりだ。\n");
    }

    #[test]
    fn fold_rules() {
        assert_eq!(fold("ＡＢ１"), "AB1");
        assert_eq!(fold("――"), fold("--"));
        assert_eq!(fold("─"), fold("――――"));
        assert_ne!(fold("ーー"), fold("――"));
        assert_eq!(fold("・・・"), fold("……"));
        assert_eq!(fold("Ⅳ"), "IV");
    }

    fn hunks2() -> Vec<Hunk> {
        vec![
            Hunk {
                before: String::new(),
                a: "竪".into(),
                b: "堅".into(),
                after: String::new(),
            },
            Hunk {
                before: String::new(),
                a: String::new(),
                b: "メテオ".into(),
                after: String::new(),
            },
        ]
    }

    #[test]
    fn parse_answers_accepts_valid_replies() {
        let reply = r#"```json
{"answers":[{"id":2,"choice":"A"},{"id":1,"choice":"B"}]}
```"#;
        assert_eq!(parse_answers(reply, &hunks2(), false).unwrap(), vec![Choice::B, Choice::A]);
        // Swapped: the model's "A" was the real B.
        assert_eq!(parse_answers(reply, &hunks2(), true).unwrap(), vec![Choice::A, Choice::B]);
    }

    #[test]
    fn parse_answers_rejects_bad_replies() {
        let h = hunks2();
        assert!(parse_answers("nope", &h, false).unwrap_err().contains("JSON"));
        let missing = r#"{"answers":[{"id":1,"choice":"A"}]}"#;
        assert!(parse_answers(missing, &h, false).unwrap_err().contains("spot 2 not answered"));
        let twice = r#"{"answers":[{"id":1,"choice":"A"},{"id":1,"choice":"B"},{"id":2,"choice":"A"}]}"#;
        assert!(parse_answers(twice, &h, false).unwrap_err().contains("twice"));
        let unknown = r#"{"answers":[{"id":3,"choice":"A"}]}"#;
        assert!(parse_answers(unknown, &h, false).unwrap_err().contains("unknown spot"));
        let other = r#"{"answers":[{"id":1,"choice":"other"},{"id":2,"choice":"A"}]}"#;
        assert!(parse_answers(other, &h, false).unwrap_err().contains("unknown choice"));
    }

    #[test]
    fn reconcile_takes_agreement_and_defaults_otherwise() {
        let h = hunks2();
        let (got, unsure) = reconcile(&h, &[Choice::B, Choice::A], &[Choice::B, Choice::B]);
        // Spot 1 agreed on B; spot 2 split, and its default (A empty) is B.
        assert_eq!(got, vec![Choice::B, Choice::B]);
        assert_eq!(unsure, 1);
    }

    #[test]
    fn user_message_numbers_hunks_and_swaps_labels() {
        let msg = arbiter_user_message(&hunks2(), false);
        assert!(msg.contains(r#""id":1"#));
        assert!(msg.contains(r#""B":"メテオ""#));
        let swapped = arbiter_user_message(&hunks2(), true);
        assert!(swapped.contains(r#""A":"メテオ""#));
    }
}
