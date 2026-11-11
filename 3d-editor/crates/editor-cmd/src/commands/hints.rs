use serde_json::{Value, json};

const MAX_DEPTH: usize = 6;

fn resolve<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
    let mut node = node;
    for _ in 0..MAX_DEPTH {
        let Some(name) = node
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix("#/$defs/"))
        else {
            break;
        };
        match root.get("$defs").and_then(|d| d.get(name)) {
            Some(target) => node = target,
            None => break,
        }
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(options)) = node.get(key)
            && let Some(first) = options
                .iter()
                .find(|o| o.get("type").and_then(Value::as_str) != Some("null"))
        {
            return resolve(root, first);
        }
    }
    node
}

fn type_name(node: &Value) -> String {
    match node.get("type") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or("value")
            .to_string(),
        _ => "value".to_string(),
    }
}

fn is_required(node: &Value, key: &str) -> bool {
    node.get("required")
        .and_then(Value::as_array)
        .is_some_and(|r| r.iter().any(|k| k == key))
}

fn shape(root: &Value, node: &Value, depth: usize) -> String {
    let node = resolve(root, node);
    if depth > MAX_DEPTH {
        return "...".into();
    }
    if let Some(Value::Array(values)) = node.get("enum") {
        return values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("|");
    }
    if let Some(Value::Array(items)) = node.get("prefixItems") {
        let parts: Vec<String> = items.iter().map(|i| shape(root, i, depth + 1)).collect();
        return format!("[{}]", parts.join(", "));
    }
    match type_name(node).as_str() {
        "object" => match node.get("properties").and_then(Value::as_object) {
            Some(props) if !props.is_empty() => {
                let parts: Vec<String> = props
                    .iter()
                    .map(|(k, v)| {
                        let optional = if is_required(node, k) { "" } else { "?" };
                        format!("{k}{optional}: {}", shape(root, v, depth + 1))
                    })
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            _ => "object".into(),
        },
        "array" => match node.get("items") {
            Some(items) => format!("[{}]", shape(root, items, depth + 1)),
            None => "array".into(),
        },
        other => other.to_string(),
    }
}

fn example(root: &Value, node: &Value, depth: usize) -> Value {
    let node = resolve(root, node);
    if depth > MAX_DEPTH {
        return Value::Null;
    }
    if let Some(Value::Array(values)) = node.get("enum")
        && let Some(first) = values.first()
    {
        return first.clone();
    }
    if let Some(Value::Array(items)) = node.get("prefixItems") {
        return Value::Array(items.iter().map(|i| example(root, i, depth + 1)).collect());
    }
    match type_name(node).as_str() {
        "object" => {
            let mut out = serde_json::Map::new();
            if let Some(props) = node.get("properties").and_then(Value::as_object) {
                for (k, v) in props.iter().filter(|(k, _)| is_required(node, k)) {
                    out.insert(k.clone(), example(root, v, depth + 1));
                }
            }
            Value::Object(out)
        }
        "array" => match node.get("items") {
            Some(items) => Value::Array(vec![example(root, items, depth + 1)]),
            None => json!([]),
        },
        "string" => json!("..."),
        "integer" | "number" => json!(0),
        "boolean" => json!(false),
        _ => Value::Null,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Key(String),
    Index,
}

/// Describes the schema node at `path` (an empty path is the whole args object).
pub fn describe(schema: &Value, path: &[Step]) -> Option<String> {
    let mut node = schema;
    for step in path {
        let current = resolve(schema, node);
        let next = match step {
            Step::Key(k) => current.get("properties").and_then(|p| p.get(k)),
            Step::Index => current
                .get("items")
                .or_else(|| current.get("prefixItems").and_then(|p| p.get(0))),
        };
        match next {
            Some(n) => node = n,
            None => break,
        }
    }
    let target = resolve(schema, node);
    Some(format!(
        "expected {}, e.g. {}",
        shape(schema, target, 0),
        example(schema, target, 0)
    ))
}

pub fn path_steps(path: &serde_path_to_error::Path) -> Vec<Step> {
    use serde_path_to_error::Segment;
    path.iter()
        .filter_map(|s| match s {
            Segment::Seq { .. } => Some(Step::Index),
            Segment::Map { key } => Some(Step::Key(key.clone())),
            Segment::Enum { .. } | Segment::Unknown => None,
        })
        .collect()
}
