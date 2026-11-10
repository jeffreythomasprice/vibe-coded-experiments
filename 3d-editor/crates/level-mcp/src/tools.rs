use std::sync::Arc;

use level_cmd::{Exec, OPS, OpInfo, Safety};
use level_host::Policy;
use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Map, Value, json};

pub fn tool_name(op: &str) -> String {
    op.replace('.', "_")
}

pub fn find_op(tool: &str) -> Option<&'static OpInfo> {
    OPS.iter().find(|o| tool_name(o.name) == tool)
}

const MAX_INLINE_DEPTH: usize = 16;

fn inline_refs(value: Value, defs: &Map<String, Value>, depth: usize) -> Value {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref")
                && let Some(name) = reference.strip_prefix("#/$defs/")
                && let Some(target) = defs.get(name)
                && depth < MAX_INLINE_DEPTH
            {
                return inline_refs(target.clone(), defs, depth + 1);
            }
            Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, inline_refs(v, defs, depth)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| inline_refs(v, defs, depth))
                .collect(),
        ),
        other => other,
    }
}

pub fn input_schema(info: &OpInfo) -> Map<String, Value> {
    let mut schema = match serde_json::to_value((info.args_schema)()) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    let defs = match schema.remove("$defs") {
        Some(Value::Object(d)) => d,
        _ => Map::new(),
    };
    schema.remove("$schema");
    schema.remove("title");
    let Value::Object(mut schema) = inline_refs(Value::Object(schema), &defs, 0) else {
        unreachable!("an object stays an object")
    };
    schema.insert("type".into(), json!("object"));

    if info.safety != Safety::Read {
        let props = schema
            .entry("properties")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("properties is an object");
        if info.exec == Exec::Document {
            props.insert(
                "dry_run".into(),
                json!({ "type": "boolean", "description": "Report what would change without applying it." }),
            );
        }
        props.insert(
            "expect_revision".into(),
            json!({ "type": "integer", "minimum": 0, "description": "Fail with RevisionMismatch unless the document is at this revision." }),
        );
    }
    schema
}

pub fn build_tool(info: &OpInfo) -> Tool {
    let mut tool = Tool::new(
        tool_name(info.name),
        info.summary,
        Arc::new(input_schema(info)),
    );
    let annotations = ToolAnnotations::new()
        .read_only(info.safety == Safety::Read)
        .destructive(info.safety == Safety::Destructive)
        .open_world(false);
    tool.annotations = Some(annotations);
    tool
}

/// Tools the policy lets a client see, in registry order.
pub fn visible_tools(policy: Policy) -> Vec<Tool> {
    OPS.iter()
        .filter(|o| policy.allows_op(o))
        .map(build_tool)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_ref(v: &Value) -> bool {
        match v {
            Value::Object(m) => m.contains_key("$ref") || m.values().any(has_ref),
            Value::Array(a) => a.iter().any(has_ref),
            _ => false,
        }
    }

    #[test]
    fn names_round_trip() {
        for op in OPS {
            assert_eq!(find_op(&tool_name(op.name)).unwrap().name, op.name);
            assert!(!tool_name(op.name).contains('.'));
        }
    }

    #[test]
    fn schemas_are_inline_objects() {
        for op in OPS {
            let schema = Value::Object(input_schema(op));
            assert_eq!(schema["type"], "object", "{}", op.name);
            assert!(!has_ref(&schema), "{}: {schema}", op.name);
        }
    }

    #[test]
    fn envelope_fields_only_on_mutations() {
        let read = input_schema(OpInfo::find("scene.get").unwrap());
        assert!(read["properties"].get("expect_revision").is_none());
        let create = input_schema(OpInfo::find("brush.create_box").unwrap());
        assert!(create["properties"].get("dry_run").is_some());
        let undo = input_schema(OpInfo::find("history.undo").unwrap());
        assert!(undo["properties"].get("dry_run").is_none());
    }

    #[test]
    fn policy_filters_tools() {
        let names = |p| -> Vec<String> {
            visible_tools(p)
                .into_iter()
                .map(|t| t.name.to_string())
                .collect()
        };
        let read = names(Policy::read_only());
        assert!(read.contains(&"scene_summary".to_string()));
        assert!(!read.contains(&"brush_create_box".to_string()));
        let mutate = names(Policy::new(true, false));
        assert!(mutate.contains(&"brush_create_box".to_string()));
        assert!(!mutate.contains(&"object_delete".to_string()));
        assert!(names(Policy::new(false, true)).contains(&"brush_create_box".to_string()));
    }
}
