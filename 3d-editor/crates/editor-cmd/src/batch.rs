use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::commands::results::BatchResults;
use crate::commands::{CmdResult, Exec, OpInfo, Outcome, dispatch};
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub commands: Vec<BatchItem>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchItem {
    /// Op name, dotted (`brush.create_box`) or underscored (`brush_create_box`).
    pub op: String,
    #[serde(default)]
    pub args: Value,
    /// Name later items can reference as `"$name"`, `"$name.ids[k]"` or `"$name.map[id]"`.
    #[serde(rename = "as")]
    pub name: Option<String>,
}

const REF_HINT: &str = "references are \"$N\" or \"$name\" (the item's `as`), optionally followed by .ids[k], .map[id] (all of what an op produced from that id, spliced into a list) or .map[id][k], and /f<n> or /v<n>; N counts from 0; write \"$$\" for a literal $";

struct Results<'a> {
    values: &'a [Value],
    names: &'a HashMap<String, usize>,
}

fn bad(msg: String) -> CommandError {
    CommandError::invalid_arg("args", msg).with_hint(REF_HINT)
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn element_suffix(suffix: &str) -> bool {
    suffix
        .strip_prefix(['f', 'v'])
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

enum Resolved {
    One(String),
    Many(Vec<String>),
}

fn bracket(tail: &str) -> Option<(&str, &str)> {
    let rest = tail.strip_prefix('[')?;
    rest.split_once(']')
}

fn resolve(reference: &str, results: &Results) -> Result<Option<Resolved>, CommandError> {
    let Some(rest) = reference.strip_prefix('$') else {
        return Ok(None);
    };
    let (head, suffix) = match rest.split_once('/') {
        Some((h, s)) => (h, Some(s)),
        None => (rest, None),
    };
    let (base, tail) = match head.split_once('.') {
        Some((b, t)) => (b, Some(t)),
        None => (head, None),
    };
    let index = if !base.is_empty() && base.bytes().all(|b| b.is_ascii_digit()) {
        base.parse::<usize>()
            .map_err(|_| bad(format!("bad index in {reference}")))?
    } else if valid_name(base) {
        *results.names.get(base).ok_or_else(|| {
            let mut defined: Vec<&str> = results.names.keys().map(String::as_str).collect();
            defined.sort_unstable();
            bad(format!("{reference} names no earlier command")).with_hint(format!(
                "defined names: [{}]; name a command with \"as\"; write \"$$\" for a literal $",
                defined.join(", ")
            ))
        })?
    } else {
        return Ok(None);
    };
    if let Some(suffix) = suffix
        && !element_suffix(suffix)
    {
        return Err(bad(format!("bad element suffix in {reference}")));
    }
    let result = results.values.get(index).ok_or_else(|| {
        bad(format!(
            "{reference} refers to a command that has not run yet"
        ))
    })?;
    let index_of = |k: &str| -> Result<usize, CommandError> {
        k.parse()
            .map_err(|_| bad(format!("bad index in {reference}")))
    };
    let found = match tail {
        None => result.get("id"),
        Some(tail) => {
            if let Some(after) = tail.strip_prefix("ids") {
                let (k, extra) =
                    bracket(after).ok_or_else(|| bad(format!("bad .ids[k] in {reference}")))?;
                if !extra.is_empty() {
                    return Err(bad(format!("unexpected text after .ids[k] in {reference}")));
                }
                let k = index_of(k)?;
                result.get("ids").and_then(|ids| ids.get(k))
            } else if let Some(after) = tail.strip_prefix("map") {
                let (key, extra) =
                    bracket(after).ok_or_else(|| bad(format!("bad .map[id] in {reference}")))?;
                let entry = result.get("map").and_then(|m| m.get(key)).ok_or_else(|| {
                    bad(format!(
                        "{reference}: the result has no map entry for `{key}`"
                    ))
                })?;
                match (entry, extra) {
                    (Value::Array(items), "") => {
                        let ids = items
                            .iter()
                            .map(|v| v.as_str().map(|id| with_suffix(id, suffix)))
                            .collect::<Option<Vec<_>>>()
                            .ok_or_else(|| bad(format!("{reference} did not produce ids")))?;
                        return Ok(Some(Resolved::Many(ids)));
                    }
                    (Value::Array(items), extra) => {
                        let (k, rest) = bracket(extra)
                            .ok_or_else(|| bad(format!("bad .map[id][k] in {reference}")))?;
                        if !rest.is_empty() {
                            return Err(bad(format!("unexpected text in {reference}")));
                        }
                        items.get(index_of(k)?)
                    }
                    (single, "") => Some(single),
                    _ => return Err(bad(format!("{reference}: that map entry is not a list"))),
                }
            } else {
                return Err(bad(format!("unknown reference path in {reference}")));
            }
        }
    };
    let id = found
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{reference} did not produce an id")))?;
    Ok(Some(Resolved::One(with_suffix(id, suffix))))
}

fn with_suffix(id: &str, suffix: Option<&str>) -> String {
    match suffix {
        Some(s) => format!("{id}/{s}"),
        None => id.to_string(),
    }
}

fn substitute_string(s: String, results: &Results) -> Result<Value, CommandError> {
    if let Some(escaped) = s.strip_prefix("$$") {
        return Ok(Value::String(format!("${escaped}")));
    }
    Ok(match resolve(&s, results)? {
        Some(Resolved::One(id)) => Value::String(id),
        Some(Resolved::Many(ids)) => Value::Array(ids.into_iter().map(Value::String).collect()),
        None => Value::String(s),
    })
}

fn substitute(value: Value, results: &Results) -> Result<Value, CommandError> {
    match value {
        Value::String(s) => substitute_string(s, results),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(s) => match substitute_string(s, results)? {
                        Value::Array(many) => out.extend(many),
                        one => out.push(one),
                    },
                    other => out.push(substitute(other, results)?),
                }
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| substitute(v, results).map(|v| (k, v)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        other => Ok(other),
    }
}

pub fn run(ctx: &mut Ctx, batch: Batch) -> CmdResult<BatchResults> {
    let mut results: Vec<Value> = Vec::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut warnings = Vec::new();
    for (index, item) in batch.commands.into_iter().enumerate() {
        let wrap = |source: CommandError| CommandError::InBatch {
            index,
            source: Box::new(source),
        };
        let op = OpInfo::find(&item.op).map_or(item.op.as_str(), |o| o.name);
        if let Some(name) = &item.name {
            if !valid_name(name) {
                return Err(wrap(
                    CommandError::invalid_arg("as", format!("`{name}` is not a valid name"))
                        .with_hint(
                            "use letters, digits and underscores, not starting with a digit",
                        ),
                ));
            }
            if names.insert(name.clone(), index).is_some() {
                return Err(wrap(
                    CommandError::invalid_arg("as", format!("name `{name}` is used twice"))
                        .with_hint("give each command a unique `as` name"),
                ));
            }
        }
        if OpInfo::find(op).is_some_and(|o| o.exec != Exec::Document) || op == "batch" {
            return Err(wrap(CommandError::invalid_arg(
                "op",
                format!("`{}` cannot run inside a batch", item.op),
            )));
        }
        let refs = Results {
            values: &results,
            names: &names,
        };
        let args = substitute(item.args, &refs).map_err(wrap)?;
        let outcome = dispatch(ctx, op, args).map_err(wrap)?;
        warnings.extend(outcome.warnings);
        results.push(outcome.result);
    }
    Ok(Outcome {
        result: BatchResults { results },
        warnings,
    })
}
