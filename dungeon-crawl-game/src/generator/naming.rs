use anyhow::{Context, Result};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::providers::ollama;
use serde::Deserialize;

use crate::schema_types::*;

#[derive(Deserialize)]
struct NameDescription {
    name: String,
    description: String,
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

fn extract_json(text: &str) -> Option<&str> {
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

async fn ask_llm(model: &str, prompt: &str) -> Result<NameDescription> {
    let client = ollama::Client::new(Nothing)?;
    let agent = client.agent(model).build();
    let response = agent.prompt(prompt).await?;
    let json = extract_json(&response).context("no JSON object found in LLM response")?;
    let result: NameDescription = serde_json::from_str(json)?;
    Ok(result)
}

pub async fn name_item(
    item: &mut Item,
    contexts: &[GeneratorContext],
    config: &Config,
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

    let result = ask_llm(&config.model, &prompt).await?;
    item.name = result.name;
    item.description = result.description;

    Ok(())
}

pub async fn name_event(
    event: &mut Event,
    contexts: &[GeneratorContext],
    config: &Config,
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

    let result = ask_llm(&config.model, &prompt).await?;
    event.name = result.name;
    event.description = result.description;

    Ok(())
}

pub async fn name_room(
    room: &mut Room,
    contexts: &[GeneratorContext],
    config: &Config,
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

    let result = ask_llm(&config.model, &prompt).await?;
    room.name = result.name;
    room.description = result.description;

    Ok(())
}
