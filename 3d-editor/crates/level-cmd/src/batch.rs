use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::{CmdResult, Exec, OPS, Outcome, dispatch};
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
    pub op: String,
    #[serde(default)]
    pub args: Value,
}

fn resolve(reference: &str, results: &[Value]) -> Result<Option<String>, CommandError> {
    let Some(rest) = reference.strip_prefix('$') else {
        return Ok(None);
    };
    let (index_part, ids_part) = match rest.split_once(".ids[") {
        Some((i, k)) => (i, Some(k.strip_suffix(']'))),
        None => (rest, None),
    };
    let Ok(index) = index_part.parse::<usize>() else {
        return Ok(None);
    };
    let bad = |msg: String| {
        CommandError::invalid_arg("args", msg)
            .with_hint("references use \"$N\" or \"$N.ids[k]\" with N counting from 0")
    };
    let result = results.get(index).ok_or_else(|| {
        bad(format!(
            "{reference} refers to a command that has not run yet"
        ))
    })?;
    let found = match ids_part {
        None => result.get("id"),
        Some(None) => return Ok(None),
        Some(Some(k)) => {
            let k: usize = k
                .parse()
                .map_err(|_| bad(format!("bad index in {reference}")))?;
            result.get("ids").and_then(|ids| ids.get(k))
        }
    };
    found
        .and_then(Value::as_str)
        .map(str::to_string)
        .map(Some)
        .ok_or_else(|| bad(format!("{reference} did not produce an id")))
}

fn substitute(value: Value, results: &[Value]) -> Result<Value, CommandError> {
    match value {
        Value::String(s) => Ok(match resolve(&s, results)? {
            Some(id) => Value::String(id),
            None => Value::String(s),
        }),
        Value::Array(items) => items
            .into_iter()
            .map(|v| substitute(v, results))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| substitute(v, results).map(|v| (k, v)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        other => Ok(other),
    }
}

pub fn run(ctx: &mut Ctx, batch: Batch) -> CmdResult {
    let mut results: Vec<Value> = Vec::new();
    let mut warnings = Vec::new();
    for (index, item) in batch.commands.into_iter().enumerate() {
        let wrap = |source: CommandError| CommandError::InBatch {
            index,
            source: Box::new(source),
        };
        if OPS
            .iter()
            .any(|o| o.name == item.op && o.exec != Exec::Document)
            || item.op == "batch"
        {
            return Err(wrap(CommandError::invalid_arg(
                "op",
                format!("`{}` cannot run inside a batch", item.op),
            )));
        }
        let args = substitute(item.args, &results).map_err(wrap)?;
        let outcome = dispatch(ctx, &item.op, args).map_err(wrap)?;
        warnings.extend(outcome.warnings);
        results.push(outcome.result);
    }
    Ok(Outcome {
        result: json!({ "results": results }),
        warnings,
    })
}
