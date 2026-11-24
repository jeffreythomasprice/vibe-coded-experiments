use std::{
    num::NonZeroU64,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::Duration,
};

use audio_recorder_schema::{
    SchemaError, from_json_str,
    llm::{
        AnthropicRequest, AnthropicResponse, ChatMessage, OllamaChatRequest, OllamaChatResponse, OllamaOptions,
    },
    to_json_string_pretty,
};

use crate::config::{ANTHROPIC_API_KEY_ENV, LlmProvider, LlmSettings};

pub const ANTHROPIC_VERSION: &str = "2026-10-21";
const ANTHROPIC_INPUT_BUDGET: u32 = 200_000;
const PROMPT_MARGIN: u32 = 64;
const CANCEL_POLL: Duration = Duration::from_millis(200);
const ERROR_BODY_LIMIT: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("request to {url} failed: {source}")]
    Http { url: String, source: Box<ureq::Error> },
    #[error("{provider} returned HTTP {status} from {url}: {body}")]
    Status { provider: &'static str, url: String, status: u16, body: String },
    #[error("unexpected response from {url}: {source}")]
    Decode { url: String, source: SchemaError },
    #[error("could not build the request: {0}")]
    Serialize(SchemaError),
    #[error("no Anthropic API key: set llm.anthropic.api_key in the config or the {ANTHROPIC_API_KEY_ENV} environment variable")]
    MissingApiKey,
    #[error("{provider} model {model} returned no text")]
    EmptyResponse { provider: &'static str, model: String },
    #[error("{provider} model {model} declined to process this content")]
    Refused { provider: &'static str, model: String },
    #[error("the {provider} response was cut off at the output limit of {max_tokens} tokens; raise llm.max_output_tokens")]
    Truncated { provider: &'static str, max_tokens: u32 },
    #[error("the prompt (about {estimated_tokens} tokens) does not fit the {num_ctx} token context; raise llm.ollama.num_ctx")]
    PromptTooLarge { estimated_tokens: u32, num_ctx: u32 },
    #[error("ollama read only {prompt_tokens} of about {estimated_tokens} prompt tokens; raise llm.ollama.num_ctx")]
    PromptTruncated { prompt_tokens: u32, estimated_tokens: u32 },
    #[error("cancelled")]
    Cancelled,
}

pub fn estimate_tokens(text: &str) -> u32 {
    let (ascii, other) = text.chars().fold((0u64, 0u64), |(a, o), c| if c.is_ascii() { (a + 1, o) } else { (a, o + 1) });
    (ascii.div_ceil(4) + other).min(u32::MAX as u64) as u32
}

pub fn input_budget(settings: &LlmSettings) -> u32 {
    match settings.provider {
        LlmProvider::Anthropic => ANTHROPIC_INPUT_BUDGET,
        LlmProvider::Ollama => settings
            .ollama
            .num_ctx
            .saturating_sub(settings.max_output_tokens)
            .saturating_sub(512)
            .max(1024),
    }
}

fn fit_output_budget(prompt_tokens: u32, num_ctx: u32, max_tokens: u32) -> Result<u32, LlmError> {
    let available = num_ctx.saturating_sub(prompt_tokens.saturating_add(PROMPT_MARGIN));
    if available < max_tokens.min(1024) {
        return Err(LlmError::PromptTooLarge { estimated_tokens: prompt_tokens, num_ctx });
    }
    Ok(max_tokens.min(available))
}

fn nonzero(value: u32) -> NonZeroU64 {
    NonZeroU64::new(u64::from(value).max(1)).expect("value is at least 1")
}

pub fn complete(settings: &LlmSettings, system: &str, user: &str, cancel: &AtomicBool) -> Result<String, LlmError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(LlmError::Cancelled);
    }
    let (tx, rx) = mpsc::channel();
    let settings = settings.clone();
    let (system, user) = (system.to_string(), user.to_string());
    std::thread::spawn(move || {
        let result = match settings.provider {
            LlmProvider::Ollama => ollama_chat(&settings, &system, &user),
            LlmProvider::Anthropic => anthropic_chat(&settings, &system, &user),
        };
        let _ = tx.send(result);
    });
    loop {
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Timeout) if cancel.load(Ordering::Relaxed) => return Err(LlmError::Cancelled),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(LlmError::EmptyResponse { provider: "llm", model: "unknown".into() });
            }
        }
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into()
}

struct Reply {
    status: u16,
    body: String,
}

fn post(timeout: Duration, url: &str, headers: &[(&str, &str)], body: &str) -> Result<Reply, LlmError> {
    let http = |source: ureq::Error| LlmError::Http { url: url.to_string(), source: Box::new(source) };
    let mut request = agent(timeout).post(url).header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send(body).map_err(http)?;
    let status = response.status().as_u16();
    let body = response.into_body().read_to_string().map_err(http)?;
    Ok(Reply { status, body })
}

fn status_error(provider: &'static str, url: &str, reply: Reply) -> LlmError {
    let mut body = reply.body.trim().to_string();
    if body.len() > ERROR_BODY_LIMIT {
        let cut = (0..=ERROR_BODY_LIMIT).rev().find(|&i| body.is_char_boundary(i)).unwrap_or(0);
        body.truncate(cut);
        body.push('…');
    }
    LlmError::Status { provider, url: url.to_string(), status: reply.status, body }
}

fn ollama_chat(settings: &LlmSettings, system: &str, user: &str) -> Result<String, LlmError> {
    let url = format!("{}/api/chat", settings.ollama.host);
    let estimated = estimate_tokens(system).saturating_add(estimate_tokens(user));
    let num_predict = fit_output_budget(estimated, settings.ollama.num_ctx, settings.max_output_tokens)?;
    let mut request = OllamaChatRequest {
        model: settings.ollama.model.clone(),
        stream: false,
        think: Some(false),
        messages: vec![
            ChatMessage { role: "system".into(), content: system.to_string() },
            ChatMessage { role: "user".into(), content: user.to_string() },
        ],
        options: OllamaOptions { num_predict: nonzero(num_predict), num_ctx: nonzero(settings.ollama.num_ctx) },
    };
    let mut reply = post(settings.timeout, &url, &[], &to_json_string_pretty(&request).map_err(LlmError::Serialize)?)?;
    if reply.status == 400 && reply.body.contains("does not support thinking") {
        request.think = None;
        reply = post(settings.timeout, &url, &[], &to_json_string_pretty(&request).map_err(LlmError::Serialize)?)?;
    }
    if !(200..300).contains(&reply.status) {
        return Err(status_error("ollama", &url, reply));
    }
    parse_ollama(&reply.body, &url, &settings.ollama.model, estimated, settings.max_output_tokens)
}

fn parse_ollama(body: &str, url: &str, model: &str, estimated: u32, max_tokens: u32) -> Result<String, LlmError> {
    let response: OllamaChatResponse =
        from_json_str(body).map_err(|source| LlmError::Decode { url: url.to_string(), source })?;
    if let Some(prompt_tokens) = response.prompt_eval_count {
        let prompt_tokens = prompt_tokens.min(u32::MAX as u64) as u32;
        if estimated >= 1024 && prompt_tokens.saturating_mul(3) < estimated {
            return Err(LlmError::PromptTruncated { prompt_tokens, estimated_tokens: estimated });
        }
    }
    let text = strip_think(&response.message.content);
    if text.is_empty() {
        return Err(LlmError::EmptyResponse { provider: "ollama", model: model.to_string() });
    }
    if response.done_reason.as_deref() == Some("length") {
        return Err(LlmError::Truncated { provider: "ollama", max_tokens });
    }
    Ok(text)
}

fn strip_think(content: &str) -> String {
    let trimmed = content.trim();
    match trimmed.strip_prefix("<think>").and_then(|rest| rest.split_once("</think>")) {
        Some((_, answer)) => answer.trim().to_string(),
        None => trimmed.to_string(),
    }
}

fn anthropic_chat(settings: &LlmSettings, system: &str, user: &str) -> Result<String, LlmError> {
    let key = settings.anthropic.api_key.as_deref().ok_or(LlmError::MissingApiKey)?;
    let url = format!("{}/v1/messages", settings.anthropic.base_url);
    let request = AnthropicRequest {
        model: settings.anthropic.model.clone(),
        max_tokens: nonzero(settings.max_output_tokens),
        system: system.to_string(),
        messages: vec![ChatMessage { role: "user".into(), content: user.to_string() }],
    };
    let body = to_json_string_pretty(&request).map_err(LlmError::Serialize)?;
    let reply = post(settings.timeout, &url, &[("x-api-key", key), ("anthropic-version", ANTHROPIC_VERSION)], &body)?;
    if !(200..300).contains(&reply.status) {
        return Err(status_error("anthropic", &url, reply));
    }
    parse_anthropic(&reply.body, &url, &settings.anthropic.model, settings.max_output_tokens)
}

fn parse_anthropic(body: &str, url: &str, model: &str, max_tokens: u32) -> Result<String, LlmError> {
    let response: AnthropicResponse =
        from_json_str(body).map_err(|source| LlmError::Decode { url: url.to_string(), source })?;
    match response.stop_reason.as_deref() {
        Some("refusal") => return Err(LlmError::Refused { provider: "anthropic", model: model.to_string() }),
        Some("max_tokens") => return Err(LlmError::Truncated { provider: "anthropic", max_tokens }),
        _ => {}
    }
    let text = response
        .content
        .iter()
        .filter(|block| block.type_ == "text")
        .filter_map(|block| block.text.as_deref())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Err(LlmError::EmptyResponse { provider: "anthropic", model: model.to_string() });
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_counts_non_ascii_per_char() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("日本語"), 3);
    }

    #[test]
    fn output_budget_shrinks_and_rejects() {
        assert_eq!(fit_output_budget(1000, 32768, 8000).unwrap(), 8000);
        assert_eq!(fit_output_budget(30000, 32768, 8000).unwrap(), 32768 - 30000 - PROMPT_MARGIN);
        assert!(matches!(fit_output_budget(32000, 32768, 8000), Err(LlmError::PromptTooLarge { .. })));
        assert_eq!(fit_output_budget(32000, 32768, 500).unwrap(), 500);
    }

    #[test]
    fn input_budget_leaves_room_for_output() {
        let mut settings = LlmSettings::default();
        settings.provider = LlmProvider::Ollama;
        settings.ollama.num_ctx = 32768;
        settings.max_output_tokens = 8000;
        assert_eq!(input_budget(&settings), 32768 - 8000 - 512);
        settings.ollama.num_ctx = 2048;
        assert_eq!(input_budget(&settings), 1024);
        settings.provider = LlmProvider::Anthropic;
        assert_eq!(input_budget(&settings), ANTHROPIC_INPUT_BUDGET);
    }

    #[test]
    fn anthropic_joins_text_blocks_and_skips_others() {
        let body = r#"{ "id": "m", "stop_reason": "end_turn", "content": [
            { "type": "thinking", "thinking": "" }, { "type": "text", "text": "  one " }, { "type": "text", "text": "two" } ] }"#;
        assert_eq!(parse_anthropic(body, "u", "m", 100).unwrap(), "one\ntwo");
    }

    #[test]
    fn anthropic_refusal_and_truncation_are_errors() {
        let refusal = r#"{ "stop_reason": "refusal", "content": [] }"#;
        assert!(matches!(parse_anthropic(refusal, "u", "m", 100), Err(LlmError::Refused { .. })));
        let cut = r#"{ "stop_reason": "max_tokens", "content": [ { "type": "text", "text": "partial" } ] }"#;
        assert!(matches!(parse_anthropic(cut, "u", "m", 100), Err(LlmError::Truncated { max_tokens: 100, .. })));
        let empty = r#"{ "stop_reason": "end_turn", "content": [] }"#;
        assert!(matches!(parse_anthropic(empty, "u", "m", 100), Err(LlmError::EmptyResponse { .. })));
        assert!(matches!(parse_anthropic("{}", "u", "m", 100), Err(LlmError::Decode { .. })));
    }

    #[test]
    fn ollama_strips_think_and_flags_problems() {
        let ok = r#"{ "message": { "role": "assistant", "content": "<think>hmm</think>\n\nAnswer" }, "done": true, "done_reason": "stop", "prompt_eval_count": 50 }"#;
        assert_eq!(parse_ollama(ok, "u", "m", 40, 100).unwrap(), "Answer");
        let cut = r#"{ "message": { "content": "partial" }, "done_reason": "length" }"#;
        assert!(matches!(parse_ollama(cut, "u", "m", 40, 100), Err(LlmError::Truncated { .. })));
        let empty = r#"{ "message": { "content": "<think>only thoughts</think>" }, "done_reason": "stop" }"#;
        assert!(matches!(parse_ollama(empty, "u", "m", 40, 100), Err(LlmError::EmptyResponse { .. })));
        let dropped = r#"{ "message": { "content": "x" }, "prompt_eval_count": 100 }"#;
        assert!(matches!(parse_ollama(dropped, "u", "m", 5000, 100), Err(LlmError::PromptTruncated { .. })));
    }

    #[test]
    fn anthropic_request_has_the_expected_shape() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut data = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = stream.read(&mut buf).unwrap();
                data.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&data).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if body.len() >= length {
                        break;
                    }
                }
            }
            let payload = r#"{ "stop_reason": "end_turn", "content": [ { "type": "text", "text": "ok" } ] }"#;
            let response = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}", payload.len());
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8_lossy(&data).to_string()
        });
        let mut settings = LlmSettings::default();
        settings.provider = LlmProvider::Anthropic;
        settings.anthropic.api_key = Some("test-key".into());
        settings.anthropic.base_url = base_url;
        settings.max_output_tokens = 1234;
        assert_eq!(complete(&settings, "be brief", "hello", &AtomicBool::new(false)).unwrap(), "ok");
        let request = server.join().unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/messages "));
        assert!(lower.contains("x-api-key: test-key"));
        assert!(lower.contains(&format!("anthropic-version: {ANTHROPIC_VERSION}")));
        let body: AnthropicRequest = from_json_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body.model, "claude-sonnet-5-5");
        assert_eq!(body.max_tokens.get(), 1234);
        assert_eq!(body.system, "be brief");
        assert_eq!((body.messages[0].role.as_str(), body.messages[0].content.as_str()), ("user", "hello"));
    }

    #[test]
    fn missing_key_is_reported_before_any_request() {
        let mut settings = LlmSettings::default();
        settings.provider = LlmProvider::Anthropic;
        settings.anthropic.api_key = None;
        let err = complete(&settings, "s", "u", &AtomicBool::new(false)).unwrap_err();
        assert!(matches!(err, LlmError::MissingApiKey), "{err}");
    }

    #[test]
    fn cancelled_before_start() {
        let err = complete(&LlmSettings::default(), "s", "u", &AtomicBool::new(true)).unwrap_err();
        assert!(matches!(err, LlmError::Cancelled));
    }
}
