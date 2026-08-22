//! `ollama.com/library` HTML scraping.
//!
//! There is no JSON API for this — verified live against the running
//! service:
//!
//! | Endpoint | Result |
//! |---|---|
//! | `registry.ollama.ai/v2/_catalog` | 404 |
//! | `registry.ollama.ai/v2/library/<model>/tags/list` | 404 |
//! | `ollama.com/api/models`, `/api/library` | 404 `{"error":"path not found"}` |
//! | `ollama.com/library` | 200, `text/html`, ~805KB, 235 model cards |
//! | `ollama.com/library/<model>/tags` | 200, `text/html` |
//!
//! So this module hand-parses the HTML, in the same spirit as [`crate::llm::sse`]
//! and [`crate::llm::ndjson`]: pure functions over `&str`, unit-tested on a
//! recorded (trimmed) fixture, no network. Unlike those two, this parser
//! tracks a specific site's markup rather than a stable wire protocol, so
//! it's more likely to need updating if the page is redesigned — a **zero
//! cards found is treated as [`LlmError::Parse`], not an empty list**, so
//! that kind of drift fails loudly (and the fixture test below would fail
//! first, since it pins the markup this parser understands).
//!
//! The tags page (`ollama.com/library/<model>/tags`) renders every tag
//! twice — once in a desktop table row, once in a `md:hidden` mobile card —
//! for the same responsive page. [`parse_tags_page`] only reads the mobile
//! card (identified by its `class="md:hidden` marker) and skips the
//! duplicate.

use shared::llm::{ModelDetails, ModelInfo};

use crate::llm::error::LlmError;

pub const PROVIDER: &str = "ollama";

/// Parse `GET {library_url}` (the model index) into one [`ModelInfo`] per
/// card, each carrying [`ModelDetails::OllamaRemote`].
pub fn parse_library(html: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let mut models = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find("<li") {
        rest = &rest[idx + "<li".len()..];
        let end = rest.find("<li").unwrap_or(rest.len());
        let card = &rest[..end];
        if let Some(model) = parse_library_card(card) {
            models.push(model);
        }
    }
    if models.is_empty() {
        return Err(LlmError::Parse {
            provider: PROVIDER,
            context: "library index".to_string(),
            message: "no model cards found — ollama.com/library markup may have changed"
                .to_string(),
        });
    }
    Ok(models)
}

fn parse_library_card(card: &str) -> Option<ModelInfo> {
    let after_href = after(card, "href=\"/library/")?;
    let (id, _) = split_at_char(after_href, '"')?;
    // An index card's href is a bare model name; a tag's href (only ever
    // seen on the tags page, not here) would carry a `:tag` suffix.
    if id.is_empty() || id.contains(':') || id.contains('/') {
        return None;
    }

    let description = after(card, "class=\"max-w-lg")
        .and_then(|s| after(s, ">"))
        .and_then(|s| split_at_char(s, '<'))
        .map(|(text, _)| decode_entities(text.trim()))
        .filter(|d| !d.is_empty());

    let mut capabilities = Vec::new();
    let mut parameter_sizes = Vec::new();
    let mut search = card;
    while let Some(s) = after(search, "inline-flex items-center rounded-md") {
        let Some(after_class) = after(s, ">") else {
            break;
        };
        let Some((text, tail)) = split_at_char(after_class, '<') else {
            break;
        };
        let text = text.trim().to_string();
        if !text.is_empty() {
            if is_parameter_size(&text) {
                parameter_sizes.push(text);
            } else {
                capabilities.push(text);
            }
        }
        search = tail;
    }

    let pulls = extract_pulls(card);
    let details = if capabilities.iter().any(|c| c == "embedding") {
        ModelDetails::OllamaRemoteEmbedding {
            description,
            capabilities,
            parameter_sizes,
            pulls,
        }
    } else {
        ModelDetails::OllamaRemote {
            description,
            capabilities,
            parameter_sizes,
            pulls,
        }
    };

    Some(ModelInfo {
        id: id.to_string(),
        display_name: None,
        created_at: None,
        details,
    })
}

/// The pull count is the last bare `<span >...</span>` (no `class`
/// attribute — every other stat span on the card has one) before the literal
/// text "Pulls".
fn extract_pulls(card: &str) -> Option<u64> {
    let pulls_idx = card.find("Pulls")?;
    let head = &card[..pulls_idx];
    let mut last_candidate: Option<String> = None;
    let mut search = head;
    while let Some(s) = after(search, "<span >") {
        let Some((text, tail)) = split_at_char(s, '<') else {
            break;
        };
        last_candidate = Some(text.trim().to_string());
        search = tail;
    }
    last_candidate.and_then(|t| parse_pull_count(&t))
}

/// Parse a pull count as ollama.com displays it: `"118.7M"`, `"3.4M"`,
/// `"2.1B"`, `"940K"`, or a bare integer with no suffix for small counts.
pub fn parse_pull_count(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let last = s.chars().last()?;
    let (digits, multiplier) = if last.is_ascii_alphabetic() {
        let multiplier = match last.to_ascii_uppercase() {
            'K' => 1_000.0,
            'M' => 1_000_000.0,
            'B' => 1_000_000_000.0,
            _ => return None,
        };
        (&s[..s.len() - last.len_utf8()], multiplier)
    } else {
        (s, 1.0)
    };
    let value: f64 = digits.parse().ok()?;
    Some((value * multiplier).round() as u64)
}

/// Whether a capability-row chip's text is a parameter-size chip (`"8b"`,
/// `"70b"`, `"405b"`, `"22m"`) rather than a capability chip (`"tools"`,
/// `"vision"`, `"embedding"`) — ollama.com's own convention for the two:
/// digits (optionally with one decimal point) followed by a single `b` or
/// `m`.
fn is_parameter_size(text: &str) -> bool {
    let Some(last) = text.chars().last() else {
        return false;
    };
    if !matches!(last.to_ascii_lowercase(), 'b' | 'm') {
        return false;
    }
    let digits = &text[..text.len() - last.len_utf8()];
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Parse `GET {library_url}/<model>/tags` into one [`ModelInfo`] per
/// pullable tag, each carrying [`ModelDetails::OllamaRemoteTag`] and an `id`
/// of the full `<model>:<tag>` string.
pub fn parse_tags_page(model: &str, html: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let prefix = format!("{model}:");
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cursor = html;

    while let Some(rest) = after(cursor, "href=\"/library/") {
        let Some((id, after_id)) = split_at_char(rest, '"') else {
            break;
        };
        cursor = after_id;

        if !id.starts_with(&prefix) {
            continue;
        }
        // The desktop table renders the exact same tag a second time, as a
        // plain `<a href="..." class="group-hover:underline">` — only the
        // `md:hidden` mobile card is parsed, so each tag is reported once.
        if !after_id.trim_start().starts_with("class=\"md:hidden") {
            continue;
        }
        if !seen.insert(id.to_string()) {
            continue;
        }

        let block = match after_id.find("</a>") {
            Some(end) => &after_id[..end],
            None => after_id,
        };
        models.push(parse_tag_block(id, block));
    }

    if models.is_empty() {
        return Err(LlmError::Parse {
            provider: PROVIDER,
            context: format!("tags page for {model}"),
            message: "no tags found — ollama.com/library/<model>/tags markup may have changed"
                .to_string(),
        });
    }
    Ok(models)
}

fn parse_tag_block(id: &str, block: &str) -> ModelInfo {
    let is_latest = block.contains(">latest<");

    let digest = after(block, "class=\"font-mono\"")
        .and_then(|s| after(s, ">"))
        .and_then(|s| split_at_char(s, '<'))
        .map(|(text, _)| text.trim().to_string())
        .filter(|d| !d.is_empty());

    // The digest span is immediately followed by ` • <size> • <context
    // window>  • ` before the next span opens.
    let (size, context_window) = after(block, "class=\"font-mono\"")
        .and_then(|s| after(s, "</span>"))
        .and_then(|s| split_at_char(s, '<'))
        .map(|(text, _)| {
            let mut parts = text.split('\u{2022}').map(str::trim).filter(|p| !p.is_empty());
            (parts.next().map(String::from), parts.next().map(String::from))
        })
        .unwrap_or((None, None));

    ModelInfo {
        id: id.to_string(),
        display_name: None,
        created_at: None,
        details: ModelDetails::OllamaRemoteTag {
            digest,
            size,
            context_window,
            is_latest,
        },
    }
}

/// Turn an HTTP status and response body into a typed error for a request
/// this module issues directly (not through a provider's own JSON API) —
/// e.g. a non-200 from `library_url`. Ollama.com doesn't return a structured
/// error envelope for these, so the body (if any) is used as-is.
pub fn parse_error(status: u16, body: &str) -> LlmError {
    let kind = crate::llm::error::ApiErrorKind::from_status(status);
    let message = if body.trim().is_empty() {
        format!("HTTP {status}")
    } else {
        body.to_string()
    };
    LlmError::Status {
        provider: PROVIDER,
        status,
        kind,
        code: None,
        message,
        request_id: None,
    }
}

/// Return the slice of `haystack` starting just after the first occurrence
/// of `needle`, or `None` if it isn't present.
fn after<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    let idx = haystack.find(needle)?;
    Some(&haystack[idx + needle.len()..])
}

/// Split `s` at the first occurrence of `delim`, returning `(before, after)`
/// with `delim` itself consumed.
fn split_at_char(s: &str, delim: char) -> Option<(&str, &str)> {
    let idx = s.find(delim)?;
    Some((&s[..idx], &s[idx + delim.len_utf8()..]))
}

/// Decode the handful of HTML entities this page actually uses. Not a
/// general-purpose decoder — good enough for model names and descriptions.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARY: &str = include_str!("fixtures/library.html");
    const LIBRARY_TAGS: &str = include_str!("fixtures/library_tags.html");

    #[test]
    fn parses_every_card_on_the_index_page() {
        let models = parse_library(LIBRARY).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama3.1");
        assert_eq!(models[1].id, "all-minilm");
    }

    #[test]
    fn parses_description_capabilities_and_parameter_sizes() {
        let models = parse_library(LIBRARY).unwrap();
        match &models[0].details {
            ModelDetails::OllamaRemote {
                description,
                capabilities,
                parameter_sizes,
                pulls,
            } => {
                assert_eq!(
                    description.as_deref(),
                    Some(
                        "Llama 3.1 is a new state-of-the-art model from Meta available in 8B, \
                         70B and 405B parameter sizes."
                    )
                );
                assert_eq!(capabilities, &vec!["tools".to_string()]);
                assert_eq!(
                    parameter_sizes,
                    &vec!["8b".to_string(), "70b".to_string(), "405b".to_string()]
                );
                assert_eq!(*pulls, Some(118_700_000));
            }
            other => panic!("expected OllamaRemote details, got {other:?}"),
        }
    }

    #[test]
    fn distinguishes_a_capability_chip_from_a_parameter_size_chip() {
        let models = parse_library(LIBRARY).unwrap();
        match &models[1].details {
            ModelDetails::OllamaRemoteEmbedding {
                capabilities,
                parameter_sizes,
                ..
            } => {
                assert_eq!(capabilities, &vec!["embedding".to_string()]);
                assert_eq!(parameter_sizes, &vec!["22m".to_string(), "33m".to_string()]);
            }
            other => panic!("expected OllamaRemoteEmbedding details, got {other:?}"),
        }
    }

    #[test]
    fn a_card_with_an_embedding_chip_classifies_as_ollama_remote_embedding() {
        let models = parse_library(LIBRARY).unwrap();
        let minilm = models.iter().find(|m| m.id == "all-minilm").unwrap();
        assert!(minilm.details.is_embedding());

        let llama = models.iter().find(|m| m.id == "llama3.1").unwrap();
        assert!(!llama.details.is_embedding());
        assert!(matches!(llama.details, ModelDetails::OllamaRemote { .. }));
    }

    #[test]
    fn parse_pull_count_handles_every_displayed_suffix() {
        assert_eq!(parse_pull_count("118.7M"), Some(118_700_000));
        assert_eq!(parse_pull_count("2.1B"), Some(2_100_000_000));
        assert_eq!(parse_pull_count("940K"), Some(940_000));
        assert_eq!(parse_pull_count("42"), Some(42));
        assert_eq!(parse_pull_count(""), None);
    }

    #[test]
    fn a_page_with_no_model_cards_is_a_parse_error_not_an_empty_list() {
        let err = parse_library("<html><body>nothing here</body></html>").unwrap_err();
        assert!(matches!(err, LlmError::Parse { .. }));
    }

    #[test]
    fn parses_tags_including_digest_size_context_and_latest_badge() {
        let models = parse_tags_page("llama3.1", LIBRARY_TAGS).unwrap();
        let by_id = |id: &str| models.iter().find(|m| m.id == id).unwrap();

        let eight_b = by_id("llama3.1:8b");
        match &eight_b.details {
            ModelDetails::OllamaRemoteTag {
                digest,
                size,
                context_window,
                is_latest,
            } => {
                assert_eq!(digest.as_deref(), Some("46e0c10c039e"));
                assert_eq!(size.as_deref(), Some("4.9GB"));
                assert_eq!(context_window.as_deref(), Some("128K context window"));
                assert!(*is_latest, "llama3.1:8b carries the latest badge");
            }
            other => panic!("expected OllamaRemoteTag details, got {other:?}"),
        }

        let four_o_five_b = by_id("llama3.1:405b");
        match &four_o_five_b.details {
            ModelDetails::OllamaRemoteTag { is_latest, size, .. } => {
                assert!(!is_latest);
                assert_eq!(size.as_deref(), Some("243GB"));
            }
            other => panic!("expected OllamaRemoteTag details, got {other:?}"),
        }
    }

    #[test]
    fn every_returned_tag_is_prefixed_with_the_requested_model() {
        let models = parse_tags_page("llama3.1", LIBRARY_TAGS).unwrap();
        assert!(!models.is_empty());
        for model in &models {
            assert!(model.id.starts_with("llama3.1:"), "unexpected id {}", model.id);
        }
    }

    #[test]
    fn the_desktop_and_mobile_duplicate_rows_collapse_to_one_entry() {
        // LIBRARY_TAGS includes both the mobile card and the desktop table
        // row for llama3.1:70b — this must appear exactly once.
        let models = parse_tags_page("llama3.1", LIBRARY_TAGS).unwrap();
        let count = models.iter().filter(|m| m.id == "llama3.1:70b").count();
        assert_eq!(count, 1, "expected the desktop/mobile duplicate to collapse to one entry");
    }

    #[test]
    fn a_tags_page_with_no_matching_rows_is_a_parse_error() {
        let err = parse_tags_page("nonexistent-model", LIBRARY_TAGS).unwrap_err();
        assert!(matches!(err, LlmError::Parse { .. }));
    }
}
