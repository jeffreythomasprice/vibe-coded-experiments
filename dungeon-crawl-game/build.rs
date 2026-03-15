use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let schema_dir = Path::new("schemas");
    println!("cargo:rerun-if-changed=schemas");
    println!("cargo:rerun-if-changed=build.rs");

    let schema_files = [
        "effect.json",
        "config.json",
        "player.json",
        "event.json",
        "item.json",
        "room.json",
        "generator_context.json",
    ];

    let mut schemas: BTreeMap<String, Value> = BTreeMap::new();
    for name in &schema_files {
        let path = schema_dir.join(name);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
        let val: Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("parse {name}: {e}"));
        schemas.insert(name.to_string(), val);
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("schema_types.rs");

    let mut output = String::new();
    output.push_str("use serde::{Deserialize, Serialize};\n\n");

    for name in &schema_files {
        let const_name = schema_const_name(name);
        let content = fs::read_to_string(schema_dir.join(name)).unwrap();
        let escaped = content
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        output.push_str(&format!("pub const {const_name}: &str = \"{escaped}\";\n"));
    }
    output.push('\n');

    output.push_str("fn default_true() -> bool { true }\n\n");

    // effect.json: definitions only
    emit_definitions(&schemas["effect.json"], &schemas, &mut output);

    for name in &schema_files[1..] {
        let schema = &schemas[*name];
        emit_definitions(schema, &schemas, &mut output);
        if schema.get("properties").is_some() {
            let type_name = schema
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or_else(|| panic!("{name} missing title"));
            emit_inline_types(type_name, schema, &schemas, &mut output);
            emit_struct(type_name, schema, &schemas, &mut output);
        }
    }

    fs::write(&out_path, &output).unwrap();
}

fn schema_const_name(filename: &str) -> String {
    let base = filename.strip_suffix(".json").unwrap();
    format!("SCHEMA_{}", base.to_uppercase().replace('-', "_"))
}

fn emit_definitions(schema: &Value, all_schemas: &BTreeMap<String, Value>, output: &mut String) {
    if let Some(defs) = schema.get("definitions").and_then(|d| d.as_object()) {
        for (name, def) in defs {
            if def.get("properties").is_some() {
                emit_inline_types(name, def, all_schemas, output);
                emit_struct(name, def, all_schemas, output);
            }
        }
    }
}

fn emit_inline_types(
    parent_name: &str,
    schema: &Value,
    all_schemas: &BTreeMap<String, Value>,
    output: &mut String,
) {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return,
    };

    for (field_name, field_schema) in props {
        if field_schema.get("$ref").is_some() {
            continue;
        }

        if let Some(one_of) = field_schema.get("oneOf").and_then(|o| o.as_array()) {
            let enum_name = format!("{parent_name}{}", to_pascal_case(field_name));
            emit_one_of_enum(&enum_name, one_of, all_schemas, output);
            continue;
        }

        let ty = field_schema
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("string");

        if ty == "string" {
            if let Some(variants) = field_schema.get("enum").and_then(|e| e.as_array()) {
                let enum_name = format!("{parent_name}{}", to_pascal_case(field_name));
                emit_enum(&enum_name, variants, field_schema, output);
            }
        } else if ty == "object" && field_schema.get("properties").is_some() {
            let struct_name = format!("{parent_name}{}", to_pascal_case(field_name));
            emit_inline_types(&struct_name, field_schema, all_schemas, output);
            emit_struct(&struct_name, field_schema, all_schemas, output);
        }
    }
}

fn emit_struct(
    name: &str,
    schema: &Value,
    all_schemas: &BTreeMap<String, Value>,
    output: &mut String,
) {
    let props = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .unwrap();
    let required: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let has_all_defaults = props.iter().all(|(field_name, field_schema)| {
        field_schema.get("default").is_some() || !required.contains(&field_name.as_str())
    });

    output.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    output.push_str(&format!("pub struct {name} {{\n"));

    for (field_name, field_schema) in props {
        let is_required = required.contains(&field_name.as_str());
        let has_default = field_schema.get("default").is_some();
        let rust_type = resolve_type(field_schema, all_schemas, name, field_name);

        if has_default {
            let attr = default_serde_attr(field_schema, &rust_type);
            output.push_str(&format!("    {attr}\n"));
        }

        if is_required || has_default {
            output.push_str(&format!("    pub {field_name}: {rust_type},\n"));
        } else {
            output.push_str(&format!("    pub {field_name}: Option<{rust_type}>,\n"));
        }
    }

    output.push_str("}\n\n");

    if has_all_defaults {
        emit_default_impl(name, props, &required, all_schemas, output);
    }
}

fn emit_default_impl(
    name: &str,
    props: &serde_json::Map<String, Value>,
    required: &[&str],
    all_schemas: &BTreeMap<String, Value>,
    output: &mut String,
) {
    output.push_str(&format!("impl Default for {name} {{\n"));
    output.push_str("    fn default() -> Self {\n");
    output.push_str("        Self {\n");

    for (field_name, field_schema) in props {
        let is_required = required.contains(&field_name.as_str());
        if let Some(default_val) = field_schema.get("default") {
            let rust_type = resolve_type(field_schema, all_schemas, name, field_name);
            let val = default_value_literal(default_val, &rust_type);
            output.push_str(&format!("            {field_name}: {val},\n"));
        } else if !is_required {
            output.push_str(&format!("            {field_name}: None,\n"));
        }
    }

    output.push_str("        }\n");
    output.push_str("    }\n");
    output.push_str("}\n\n");
}

const RUST_KEYWORDS: &[&str] = &[
    "self", "Self", "super", "crate", "type", "fn", "mod", "use", "pub", "let", "mut", "ref",
    "const", "static", "extern", "as", "break", "continue", "else", "enum", "false", "for", "if",
    "impl", "in", "loop", "match", "move", "return", "struct", "trait", "true", "unsafe", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield",
];

fn emit_enum(name: &str, variants: &[Value], schema: &Value, output: &mut String) {
    output.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    output.push_str("#[serde(rename_all = \"snake_case\")]\n");
    output.push_str(&format!("pub enum {name} {{\n"));

    for v in variants {
        let s = v.as_str().unwrap();
        let variant = to_pascal_case(s);
        if RUST_KEYWORDS.contains(&variant.as_str()) || variant == "Self" {
            output.push_str(&format!("    #[serde(rename = \"{s}\")]\n"));
            output.push_str(&format!("    {variant}Kind,\n"));
        } else {
            output.push_str(&format!("    {variant},\n"));
        }
    }

    output.push_str("}\n\n");

    if let Some(default_val) = schema.get("default").and_then(|d| d.as_str()) {
        let variant = to_pascal_case(default_val);
        output.push_str(&format!("impl Default for {name} {{\n"));
        output.push_str(&format!(
            "    fn default() -> Self {{ Self::{variant} }}\n"
        ));
        output.push_str("}\n\n");
    }
}

fn emit_one_of_enum(
    name: &str,
    variants: &[Value],
    all_schemas: &BTreeMap<String, Value>,
    output: &mut String,
) {
    output.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    output.push_str("#[serde(untagged)]\n");
    output.push_str(&format!("pub enum {name} {{\n"));

    for v in variants {
        if let Some(r) = v.get("$ref").and_then(|r| r.as_str()) {
            let type_name = resolve_one_of_ref(r, all_schemas);
            output.push_str(&format!("    {type_name}({type_name}),\n"));
        }
    }

    output.push_str("}\n\n");
}

fn resolve_one_of_ref(r: &str, all_schemas: &BTreeMap<String, Value>) -> String {
    if r.contains("#/definitions/") {
        return resolve_ref(r);
    }
    // Top-level schema file ref like "item.json" — use its title
    if let Some(schema) = all_schemas.get(r) {
        if let Some(title) = schema.get("title").and_then(|t| t.as_str()) {
            return title.to_string();
        }
    }
    to_pascal_case(r.strip_suffix(".json").unwrap_or(r))
}

fn resolve_type(
    schema: &Value,
    all_schemas: &BTreeMap<String, Value>,
    parent_name: &str,
    field_name: &str,
) -> String {
    if let Some(r) = schema.get("$ref").and_then(|r| r.as_str()) {
        return resolve_ref(r);
    }

    if schema.get("oneOf").is_some() {
        return format!("{parent_name}{}", to_pascal_case(field_name));
    }

    let ty = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");

    match ty {
        "string" => {
            if schema.get("enum").is_some() {
                format!("{parent_name}{}", to_pascal_case(field_name))
            } else {
                "String".to_string()
            }
        }
        "integer" => "i64".to_string(),
        "number" => "f64".to_string(),
        "boolean" => "bool".to_string(),
        "array" => {
            let items = schema.get("items").unwrap();
            let inner = resolve_type(items, all_schemas, parent_name, field_name);
            format!("Vec<{inner}>")
        }
        "object" => {
            if schema.get("properties").is_some() {
                format!("{parent_name}{}", to_pascal_case(field_name))
            } else {
                "serde_json::Value".to_string()
            }
        }
        _ => "serde_json::Value".to_string(),
    }
}

fn resolve_ref(r: &str) -> String {
    let def_part = if let Some(idx) = r.find("#/definitions/") {
        &r[idx + "#/definitions/".len()..]
    } else {
        r
    };
    def_part.to_string()
}

fn default_serde_attr(schema: &Value, rust_type: &str) -> String {
    let default_val = schema.get("default").unwrap();
    match default_val {
        Value::Bool(true) => "#[serde(default = \"default_true\")]".to_string(),
        Value::String(s) if !s.is_empty() && rust_type != "String" => {
            // Enum default — serde(default) won't work, need a fn.
            // But we generate Default impl for the enum, so serde(default) works.
            "#[serde(default)]".to_string()
        }
        _ => "#[serde(default)]".to_string(),
    }
}

fn default_value_literal(val: &Value, rust_type: &str) -> String {
    match val {
        Value::Number(n) => {
            if rust_type == "f64" {
                format!("{}_f64", n)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => {
            if rust_type != "String" {
                let variant = to_pascal_case(s);
                let variant = if RUST_KEYWORDS.contains(&variant.as_str()) || variant == "Self" {
                    format!("{variant}Kind")
                } else {
                    variant
                };
                format!("{rust_type}::{variant}")
            } else {
                format!("\"{s}\".to_string()")
            }
        }
        Value::Bool(b) => format!("{b}"),
        _ => "Default::default()".to_string(),
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{upper}{}", chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect()
}
