use anyhow::{Context, Result};
use async_trait::async_trait;
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::providers::ollama;
use serde::Deserialize;

use crate::schema_types::*;
use super::LlmClient;

#[derive(Deserialize)]
struct NameDescription {
    name: String,
    description: String,
}

pub struct OllamaClient;

#[async_trait]
impl LlmClient for OllamaClient {
    async fn complete(&self, model: &str, prompt: &str) -> Result<String> {
        let client = ollama::Client::new(Nothing)?;
        let agent = client.agent(model).build();
        let response = agent.prompt(prompt).await?;
        Ok(response)
    }
}

fn filter_context(contexts: &[GeneratorContext], kind: ContextEntryAppliesTo) -> String {
    let mut parts = Vec::new();
    for ctx in contexts {
        for entry in &ctx.entries {
            match &entry.applies_to {
                ContextEntryAppliesTo::All => parts.push(entry.text.as_str()),
                other if std::mem::discriminant(other) == std::mem::discriminant(&kind) => {
                    parts.push(entry.text.as_str());
                }
                _ => {}
            }
        }
    }
    parts.join("\n")
}

pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

async fn ask_llm(llm: &dyn LlmClient, model: &str, prompt: &str) -> Result<NameDescription> {
    let response = llm.complete(model, prompt).await?;
    let json = extract_json(&response).context("no JSON object found in LLM response")?;
    let result: NameDescription = serde_json::from_str(json)?;
    Ok(result)
}

pub fn is_too_similar(name: &str, existing: &[String]) -> bool {
    let lower = name.to_lowercase();
    existing.iter().any(|e| {
        let el = e.to_lowercase();
        el == lower || el.contains(&lower) || lower.contains(&el)
    })
}

fn avoid_duplicates_clause(existing: &[String]) -> String {
    if existing.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nDo not reuse or closely resemble any of these existing names: {}",
            existing.join(", ")
        )
    }
}

async fn ask_llm_unique(
    llm: &dyn LlmClient,
    model: &str,
    base_prompt: &str,
    existing: &[String],
) -> Result<NameDescription> {
    let prompt = format!("{base_prompt}{}", avoid_duplicates_clause(existing));
    for attempt in 0..3 {
        let result = ask_llm(llm, model, &prompt).await?;
        if attempt < 2 && is_too_similar(&result.name, existing) {
            continue;
        }
        return Ok(result);
    }
    unreachable!()
}

pub async fn name_item(
    item: &mut Item,
    contexts: &[GeneratorContext],
    config: &Config,
    existing_names: &[String],
    llm: &dyn LlmClient,
) -> Result<()> {
    let ctx = filter_context(contexts, ContextEntryAppliesTo::Item);
    let skeleton = serde_json::to_string_pretty(item)?;

    let prompt = format!(
        "You are a creative game designer naming items for a dungeon crawl game.\n\
         \n\
         Theme context:\n{ctx}\n\
         \n\
         Here is the mechanical skeleton of an item:\n{skeleton}\n\
         \n\
         Respond with ONLY a JSON object with two fields:\n\
         - \"name\": a short, evocative item name\n\
         - \"description\": a one-sentence description of the item"
    );

    let result = ask_llm_unique(llm, &config.model, &prompt, existing_names).await?;
    item.name = result.name;
    item.description = result.description;

    Ok(())
}

pub async fn name_event(
    event: &mut Event,
    contexts: &[GeneratorContext],
    config: &Config,
    existing_names: &[String],
    llm: &dyn LlmClient,
) -> Result<()> {
    let ctx = filter_context(contexts, ContextEntryAppliesTo::Event);
    let skeleton = serde_json::to_string_pretty(event)?;

    let prompt = format!(
        "You are a creative game designer naming events for a dungeon crawl game.\n\
         \n\
         Theme context:\n{ctx}\n\
         \n\
         Here is the mechanical skeleton of an event:\n{skeleton}\n\
         \n\
         Respond with ONLY a JSON object with two fields:\n\
         - \"name\": a short, evocative event name\n\
         - \"description\": a one-sentence description of the event"
    );

    let result = ask_llm_unique(llm, &config.model, &prompt, existing_names).await?;
    event.name = result.name;
    event.description = result.description;

    Ok(())
}

pub async fn name_player(
    player: &mut Player,
    contexts: &[GeneratorContext],
    config: &Config,
    existing_names: &[String],
    llm: &dyn LlmClient,
) -> Result<()> {
    let ctx = filter_context(contexts, ContextEntryAppliesTo::Player);
    let skeleton = serde_json::to_string_pretty(player)?;

    let prompt = format!(
        "You are a creative game designer naming player characters for a dungeon crawl game.\n\
         \n\
         Theme context:\n{ctx}\n\
         \n\
         Here is the mechanical skeleton of a player character:\n{skeleton}\n\
         \n\
         Respond with ONLY a JSON object with two fields:\n\
         - \"name\": a short, evocative character name\n\
         - \"description\": a one-sentence description of the character"
    );

    let result = ask_llm_unique(llm, &config.model, &prompt, existing_names).await?;
    player.name = result.name;
    player.description = result.description;

    Ok(())
}

pub async fn name_room(
    room: &mut Room,
    contexts: &[GeneratorContext],
    config: &Config,
    existing_names: &[String],
    llm: &dyn LlmClient,
) -> Result<()> {
    let ctx = filter_context(contexts, ContextEntryAppliesTo::Room);
    let skeleton = serde_json::to_string_pretty(room)?;

    let prompt = format!(
        "You are a creative game designer writing room descriptions for a dungeon crawl game.\n\
         \n\
         Theme context:\n{ctx}\n\
         \n\
         Here is the mechanical skeleton of a room:\n{skeleton}\n\
         \n\
         Respond with ONLY a JSON object with two fields:\n\
         - \"name\": a short, evocative room name\n\
         - \"description\": a two-sentence atmospheric description of the room"
    );

    let result = ask_llm_unique(llm, &config.model, &prompt, existing_names).await?;
    room.name = result.name;
    room.description = result.description;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_valid() {
        let text = r#"Here is the JSON: {"name": "Sword", "description": "A sharp blade"} done"#;
        let json = extract_json(text).unwrap();
        assert_eq!(json, r#"{"name": "Sword", "description": "A sharp blade"}"#);
    }

    #[test]
    fn extract_json_nested() {
        let text = r#"{"outer": {"inner": 1}}"#;
        assert_eq!(extract_json(text).unwrap(), text);
    }

    #[test]
    fn extract_json_none() {
        assert!(extract_json("no json here").is_none());
    }

    #[test]
    fn extract_json_unclosed() {
        assert!(extract_json("{ unclosed").is_none());
    }

    #[test]
    fn is_too_similar_exact() {
        assert!(is_too_similar("Sword", &["sword".to_string()]));
    }

    #[test]
    fn is_too_similar_substring() {
        assert!(is_too_similar("Dark", &["The Dark Knight".to_string()]));
    }

    #[test]
    fn is_too_similar_dissimilar() {
        assert!(!is_too_similar("Sword", &["Shield".to_string()]));
    }

    #[test]
    fn is_too_similar_empty() {
        assert!(!is_too_similar("Anything", &[]));
    }

    struct MockLlm {
        responses: std::sync::Mutex<Vec<String>>,
    }

    impl MockLlm {
        fn new(responses: Vec<String>) -> Self {
            Self { responses: std::sync::Mutex::new(responses) }
        }
    }

    #[async_trait]
    impl LlmClient for MockLlm {
        async fn complete(&self, _model: &str, _prompt: &str) -> Result<String> {
            let mut resps = self.responses.lock().unwrap();
            if resps.is_empty() {
                anyhow::bail!("no more mock responses");
            }
            Ok(resps.remove(0))
        }
    }

    #[tokio::test]
    async fn name_room_with_mock() {
        let mock = MockLlm::new(vec![
            r#"{"name": "Haunted Hall", "description": "A spooky corridor."}"#.to_string(),
        ]);
        let config = Config { model: "test".to_string() };
        let mut room = Room {
            name: String::new(),
            description: String::new(),
            door_config: DoorConfig { arrangement: DoorConfigArrangement::DeadEnd },
            magnitude: 0.5,
            content: None,
        };
        name_room(&mut room, &[], &config, &[], &mock).await.unwrap();
        assert_eq!(room.name, "Haunted Hall");
        assert_eq!(room.description, "A spooky corridor.");
    }

    #[tokio::test]
    async fn name_player_with_mock() {
        let mock = MockLlm::new(vec![
            r#"{"name": "Eldric", "description": "A brave warrior."}"#.to_string(),
        ]);
        let config = Config { model: "test".to_string() };
        let mut player = Player {
            name: String::new(),
            description: String::new(),
            stats: PlayerStats::default(),
            color: None,
        };
        name_player(&mut player, &[], &config, &[], &mock).await.unwrap();
        assert_eq!(player.name, "Eldric");
    }

    #[tokio::test]
    async fn ask_llm_unique_retries_on_duplicate() {
        let mock = MockLlm::new(vec![
            r#"{"name": "Sword", "description": "dup"}"#.to_string(),
            r#"{"name": "Sword", "description": "dup"}"#.to_string(),
            r#"{"name": "Axe", "description": "unique"}"#.to_string(),
        ]);
        let existing = vec!["Sword".to_string()];
        let result = ask_llm_unique(&mock, "test", "prompt", &existing).await.unwrap();
        assert_eq!(result.name, "Axe");
    }

    #[tokio::test]
    async fn ask_llm_unique_gives_up_after_3() {
        let mock = MockLlm::new(vec![
            r#"{"name": "Sword", "description": "dup"}"#.to_string(),
            r#"{"name": "Sword", "description": "dup"}"#.to_string(),
            r#"{"name": "Sword", "description": "dup"}"#.to_string(),
        ]);
        let existing = vec!["Sword".to_string()];
        let result = ask_llm_unique(&mock, "test", "prompt", &existing).await.unwrap();
        // on 3rd attempt it accepts even if similar
        assert_eq!(result.name, "Sword");
    }
}
