//! Language codes, document-language detection, and summary-language
//! resolution.
//!
//! Everything stored in the DB (`document.language`, `document_summary.language`)
//! is a lowercase ISO 639-1 code where one exists (`en`, `ja`, …). `whatlang`
//! reports ISO 639-3 (`eng`, `jpn`, `cmn`, …), so detection maps through
//! `LANGUAGES`; languages missing from the table keep their 639-3 code.

/// Special `[summarize].languages` entry meaning "the document's detected
/// language".
pub const SOURCE: &str = "source";

/// Fallback when nothing else resolves.
pub const DEFAULT: &str = "en";

/// `(iso 639-1, whatlang iso 639-3, English name)`. Covers the languages most
/// likely to show up; anything else still works, it just round-trips as its
/// raw code and the prompt names it by code.
const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "eng", "English"),
    ("ja", "jpn", "Japanese"),
    ("zh", "cmn", "Chinese"),
    ("ko", "kor", "Korean"),
    ("fr", "fra", "French"),
    ("de", "deu", "German"),
    ("es", "spa", "Spanish"),
    ("it", "ita", "Italian"),
    ("pt", "por", "Portuguese"),
    ("nl", "nld", "Dutch"),
    ("ru", "rus", "Russian"),
    ("uk", "ukr", "Ukrainian"),
    ("pl", "pol", "Polish"),
    ("sv", "swe", "Swedish"),
    ("tr", "tur", "Turkish"),
    ("ar", "ara", "Arabic"),
    ("he", "heb", "Hebrew"),
    ("hi", "hin", "Hindi"),
    ("th", "tha", "Thai"),
    ("vi", "vie", "Vietnamese"),
];

/// How much of the document to feed the detector. whatlang is linear in
/// input size and converges long before this.
const DETECT_SAMPLE_BYTES: usize = 64 * 1024;

/// Detect the dominant language of `text`, returned as an ISO 639-1 code
/// (or 639-3 when the table has no 639-1 entry). `None` when whatlang can't
/// decide or flags its own guess as unreliable.
pub fn detect(text: &str) -> Option<String> {
    let mut end = text.len().min(DETECT_SAMPLE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let info = whatlang::detect(&text[..end])?;
    if !info.is_reliable() {
        tracing::debug!(
            lang = info.lang().code(),
            confidence = info.confidence(),
            "language: detection unreliable, leaving unset",
        );
        return None;
    }
    let iso3 = info.lang().code();
    Some(
        LANGUAGES
            .iter()
            .find(|(_, w, _)| *w == iso3)
            .map(|(c, _, _)| (*c).to_string())
            .unwrap_or_else(|| iso3.to_string()),
    )
}

/// Human-readable English name for a stored code, for prompts and display.
/// Unknown codes come back unchanged.
pub fn display_name(code: &str) -> String {
    LANGUAGES
        .iter()
        .find(|(c, w, _)| *c == code || *w == code)
        .map(|(_, _, n)| (*n).to_string())
        .unwrap_or_else(|| code.to_string())
}

/// Lowercase + trim, the canonical stored form.
pub fn normalize(code: &str) -> String {
    code.trim().to_ascii_lowercase()
}

/// Turn the configured / CLI-requested summary languages into the concrete
/// codes to build trees for. `"source"` resolves to `doc_language` (or
/// `DEFAULT` when that's unknown). Order is preserved, duplicates collapse,
/// blanks are dropped, and an empty result falls back to `[DEFAULT]`.
pub fn resolve_summary_languages(requested: &[String], doc_language: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in requested {
        let code = normalize(raw);
        if code.is_empty() {
            continue;
        }
        let resolved = if code == SOURCE {
            doc_language.map(normalize).unwrap_or_else(|| DEFAULT.to_string())
        } else {
            code
        };
        if !out.contains(&resolved) {
            out.push(resolved);
        }
    }
    if out.is_empty() {
        out.push(DEFAULT.to_string());
    }
    out
}

/// The summary language to show when the caller didn't pick one:
/// `[search].summary_language` if set, else the first resolved
/// `[summarize].languages` entry.
pub fn preferred_summary_language(
    search_language: Option<&str>,
    summarize_languages: &[String],
    doc_language: Option<&str>,
) -> String {
    match search_language {
        Some(l) => normalize(l),
        None => resolve_summary_languages(summarize_languages, doc_language).swap_remove(0),
    }
}

/// Number of trees `requested` will produce, without knowing the document
/// language yet. `"source"` is counted as a distinct tree unless it's the
/// only entry; this may over-count by one when the source matches another
/// configured code, which only makes progress/ETA slightly pessimistic.
pub fn estimate_summary_language_count(requested: &[String]) -> usize {
    let mut codes: Vec<String> = requested
        .iter()
        .map(|s| normalize(s))
        .filter(|s| !s.is_empty())
        .collect();
    codes.sort();
    codes.dedup();
    codes.len().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn resolve_source_uses_document_language() {
        assert_eq!(
            resolve_summary_languages(&s(&["en", "source"]), Some("ja")),
            s(&["en", "ja"])
        );
    }

    #[test]
    fn resolve_source_collapses_when_same_as_other_entry() {
        assert_eq!(
            resolve_summary_languages(&s(&["en", "source"]), Some("en")),
            s(&["en"])
        );
    }

    #[test]
    fn resolve_source_falls_back_to_default_when_unknown() {
        assert_eq!(resolve_summary_languages(&s(&["source"]), None), s(&["en"]));
    }

    #[test]
    fn resolve_normalizes_and_handles_empty() {
        assert_eq!(
            resolve_summary_languages(&s(&[" JA ", "ja", ""]), None),
            s(&["ja"])
        );
        assert_eq!(resolve_summary_languages(&[], None), s(&["en"]));
    }

    #[test]
    fn estimate_counts_distinct_entries() {
        assert_eq!(estimate_summary_language_count(&s(&["en", "source"])), 2);
        assert_eq!(estimate_summary_language_count(&s(&["en", "EN"])), 1);
        assert_eq!(estimate_summary_language_count(&[]), 1);
    }

    #[test]
    fn preferred_uses_search_override_then_first_resolved() {
        assert_eq!(preferred_summary_language(Some(" JA "), &s(&["en"]), None), "ja");
        assert_eq!(
            preferred_summary_language(None, &s(&["source", "en"]), Some("ja")),
            "ja"
        );
        assert_eq!(preferred_summary_language(None, &[], None), "en");
    }

    #[test]
    fn detect_japanese_and_english() {
        let ja = "吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。\
                  何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。";
        assert_eq!(detect(ja).as_deref(), Some("ja"));
        let en = "The quick brown fox jumps over the lazy dog while the committee \
                  deliberates on the budget for the upcoming fiscal year.";
        assert_eq!(detect(en).as_deref(), Some("en"));
    }

    #[test]
    fn display_name_known_and_unknown() {
        assert_eq!(display_name("ja"), "Japanese");
        assert_eq!(display_name("xx"), "xx");
    }
}
