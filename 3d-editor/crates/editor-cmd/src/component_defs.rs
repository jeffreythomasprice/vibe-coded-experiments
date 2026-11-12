use std::collections::BTreeMap;
use std::path::Path;

use editor_core::{
    ComponentDef, ComponentDefsFile, ComponentDefsId, ComponentSize, DEFS_VERSION, DefsSource,
    Document, Properties, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::links::{self, Link};
use crate::texture::{self, TextureError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefsProblemKind {
    Unreadable,
    Malformed,
    UnsupportedVersion,
    BadSchema,
    DuplicateComponent,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DefsProblem {
    #[error("{from}: cannot read {path}: {reason}")]
    Unreadable {
        from: ComponentDefsId,
        path: String,
        reason: String,
    },
    #[error("{from}: not a definitions file: {reason}")]
    Malformed {
        from: ComponentDefsId,
        reason: String,
    },
    #[error("{from}: definitions version {found} is not supported (latest is {DEFS_VERSION})")]
    UnsupportedVersion { from: ComponentDefsId, found: u32 },
    #[error("{from}: schema of `{name}` is not valid: {reason}")]
    BadSchema {
        from: ComponentDefsId,
        name: String,
        reason: String,
    },
    #[error("{from}: `{name}` is already defined by {}; {}", first_source(.first), if .first.is_none() { "the built-in wins" } else { "this definition wins" })]
    DuplicateComponent {
        from: ComponentDefsId,
        name: String,
        first: Option<ComponentDefsId>,
    },
}

impl DefsProblem {
    pub fn from(&self) -> ComponentDefsId {
        match self {
            Self::Unreadable { from, .. }
            | Self::Malformed { from, .. }
            | Self::UnsupportedVersion { from, .. }
            | Self::BadSchema { from, .. }
            | Self::DuplicateComponent { from, .. } => *from,
        }
    }

    pub fn kind(&self) -> DefsProblemKind {
        match self {
            Self::Unreadable { .. } => DefsProblemKind::Unreadable,
            Self::Malformed { .. } => DefsProblemKind::Malformed,
            Self::UnsupportedVersion { .. } => DefsProblemKind::UnsupportedVersion,
            Self::BadSchema { .. } => DefsProblemKind::BadSchema,
            Self::DuplicateComponent { .. } => DefsProblemKind::DuplicateComponent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    pub pointer: String,
    pub message: String,
}

fn first_source(first: &Option<ComponentDefsId>) -> String {
    match first {
        Some(id) => id.to_string(),
        None => "a built-in component".into(),
    }
}

/// Advisory findings about one component of an entity; never block an edit.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ComponentIssue {
    #[error("component `{name}` has no definition")]
    Unknown { name: String },
    #[error("{}: {}", .0.pointer, .0.message)]
    Schema(SchemaError),
}

pub struct Resolved {
    pub def: ComponentDef,
    /// The definitions record that provides it; `None` for a built-in.
    pub from: Option<ComponentDefsId>,
    validator: Option<editor_schema::CompiledSchema>,
}

impl std::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolved")
            .field("name", &self.def.name)
            .field("from", &self.from)
            .finish()
    }
}

struct Builtin {
    name: &'static str,
    def: &'static str,
    category: &'static str,
    color: Option<[f32; 3]>,
    size: Option<([f64; 3], [f64; 3])>,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "keyframe_animation",
        def: "KeyframeAnimation",
        category: "movers",
        color: None,
        size: None,
    },
    Builtin {
        name: "player_start",
        def: "PlayerStart",
        category: "player",
        color: Some([0.31, 0.82, 0.43]),
        size: Some(([-6.0, -6.0, -16.0], [6.0, 6.0, 16.0])),
    },
];

pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTINS.iter().map(|b| b.name)
}

fn builtin_def(b: &Builtin) -> ComponentDef {
    let all = editor_schema::bundled()["$defs"]
        .as_object()
        .expect("bundle has $defs");
    let mut schema = all[b.def].clone();
    let description = schema
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(map) = schema.as_object_mut() {
        map.insert("$defs".into(), Value::Object(all.clone()));
    }
    ComponentDef {
        name: b.name.parse().expect("built-in names are valid"),
        description,
        category: Some(b.category.to_string()),
        color: b.color,
        size: b.size.map(|(min, max)| ComponentSize { min, max }),
        schema,
    }
}

#[derive(Debug, Default)]
pub struct Catalog {
    components: BTreeMap<String, Resolved>,
    problems: Vec<DefsProblem>,
}

fn read_file(
    from: ComponentDefsId,
    path: &str,
    scene_dir: Option<&Path>,
) -> Result<ComponentDefsFile, DefsProblem> {
    let unreadable = |reason: String| DefsProblem::Unreadable {
        from,
        path: path.to_string(),
        reason,
    };
    let file = texture::resolve_path(path, scene_dir).map_err(|e| match e {
        TextureError::RelativeWithoutScene { .. } => {
            unreadable("a relative path needs the scene to be saved first".into())
        }
        e => unreadable(e.to_string()),
    })?;
    let bytes = std::fs::read(&file).map_err(|e| unreadable(format!("{}: {e}", file.display())))?;
    editor_schema::parse_slice::<ComponentDefsFile>(&bytes).map_err(|e| DefsProblem::Malformed {
        from,
        reason: e.to_string(),
    })
}

impl Catalog {
    fn builtins() -> Self {
        let mut catalog = Self::default();
        for b in BUILTINS {
            catalog.insert(None, builtin_def(b));
        }
        catalog
    }

    pub fn resolve(doc: &Document, scene_dir: Option<&Path>) -> Self {
        let mut catalog = Self::builtins();
        for defs in doc.component_defs.values() {
            let file = match &defs.source {
                DefsSource::Embedded { defs } => Ok(defs.clone()),
                DefsSource::Path { path } => read_file(defs.id, path, scene_dir),
            };
            match file {
                Ok(file) => catalog.add_file(defs.id, file),
                Err(problem) => catalog.problems.push(problem),
            }
        }
        catalog
    }

    /// Builds a catalog from already-resolved components, e.g. the result of `component_defs.list`.
    pub fn from_components(
        components: impl IntoIterator<Item = (Option<ComponentDefsId>, ComponentDef)>,
    ) -> Self {
        let mut catalog = Self::default();
        for (from, def) in components {
            catalog.insert(from, def);
        }
        catalog
    }

    fn add_file(&mut self, from: ComponentDefsId, file: ComponentDefsFile) {
        if file.version != DEFS_VERSION {
            self.problems.push(DefsProblem::UnsupportedVersion {
                from,
                found: file.version,
            });
            return;
        }
        for def in file.components {
            if let Some(first) = self.components.get(def.name.as_str()) {
                if first.from.is_none() {
                    self.problems.push(DefsProblem::DuplicateComponent {
                        from,
                        name: def.name.to_string(),
                        first: None,
                    });
                    continue;
                }
                self.problems.push(DefsProblem::DuplicateComponent {
                    from,
                    name: def.name.to_string(),
                    first: first.from,
                });
            }
            self.insert(Some(from), def);
        }
    }

    fn insert(&mut self, from: Option<ComponentDefsId>, def: ComponentDef) {
        let validator = match editor_schema::CompiledSchema::compile(&def.schema) {
            Ok(v) => Some(v),
            Err(editor_schema::SchemaCompileError::Invalid(reason)) => {
                if let Some(from) = from {
                    self.problems.push(DefsProblem::BadSchema {
                        from,
                        name: def.name.to_string(),
                        reason,
                    });
                }
                None
            }
        };
        self.components.insert(
            def.name.to_string(),
            Resolved {
                def,
                from,
                validator,
            },
        );
    }

    pub fn component(&self, name: &str) -> Option<&Resolved> {
        self.components.get(name)
    }

    pub fn components(&self) -> impl Iterator<Item = &Resolved> {
        self.components.values()
    }

    pub fn problems(&self) -> &[DefsProblem] {
        &self.problems
    }

    pub fn validate(&self, name: &str, data: &Properties) -> Vec<SchemaError> {
        let Some(validator) = self.components.get(name).and_then(|c| c.validator.as_ref()) else {
            return Vec::new();
        };
        validator
            .validate(&Value::Object(data.clone()))
            .into_iter()
            .map(|v| SchemaError {
                pointer: v.pointer,
                message: v.message,
            })
            .collect()
    }

    /// Unknown components are only reported once a definitions file contributes components, so a scene without any stays quiet.
    pub fn component_issues(&self, name: &str, data: &Properties) -> Vec<ComponentIssue> {
        if !self.components.contains_key(name) {
            let custom = self.components.values().any(|c| c.from.is_some());
            return if custom {
                vec![ComponentIssue::Unknown {
                    name: name.to_string(),
                }]
            } else {
                Vec::new()
            };
        }
        self.validate(name, data)
            .into_iter()
            .map(ComponentIssue::Schema)
            .collect()
    }

    /// Top-level `properties.*.default` values of the component schema.
    pub fn defaults(&self, name: &str) -> Properties {
        let mut out = Properties::new();
        let Some(props) = self
            .components
            .get(name)
            .and_then(|c| c.def.schema.get("properties"))
            .and_then(Value::as_object)
        else {
            return out;
        };
        for (key, schema) in props {
            if let Some(default) = schema.get("default")
                && editor_core::find_null(default).is_none()
            {
                out.insert(key.clone(), default.clone());
            }
        }
        out
    }

    pub fn links(&self, name: &str, data: &Properties) -> Vec<Link> {
        match self.components.get(name) {
            Some(r) => links::links_of(&r.def.schema, &Value::Object(data.clone())),
            None => Vec::new(),
        }
    }
}
