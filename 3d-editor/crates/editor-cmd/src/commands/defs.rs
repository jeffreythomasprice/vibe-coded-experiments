use std::path::Path;

use editor_core::{
    ComponentDefs, ComponentDefsFile, ComponentDefsId, DefsSource, Extra, Object, ObjectId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::material::Store;
use super::results::{self, ComponentInfo, DefsProblemView, DefsRecordInfo, DefsSourceKind};
use super::util::non_empty;
use super::{CmdResult, NoArgs, Outcome};
use crate::component_defs::Catalog;
use crate::ctx::Ctx;
use crate::error::CommandError;
use crate::texture;

/// Largest definitions file that may be embedded or referenced.
pub const MAX_DEFS_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum Add {
    /// A definitions file (`*.components.json`) on disk. Relative `path`s resolve against the working directory of the client.
    File {
        path: String,
        /// How to record it; `relative` when the scene is saved, otherwise `absolute`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        store: Option<Store>,
    },
    /// The definitions themselves.
    Embedded { defs: ComponentDefsFile },
}

fn bad(message: impl Into<String>) -> CommandError {
    CommandError::invalid_arg("path", message)
}

fn parse_file(file: &Path) -> Result<ComponentDefsFile, CommandError> {
    let bytes = std::fs::read(file).map_err(|e| bad(format!("{}: {e}", file.display())))?;
    if bytes.len() > MAX_DEFS_BYTES {
        return Err(bad(format!("{} is larger than 16 MiB", file.display())));
    }
    editor_schema::parse_slice::<ComponentDefsFile>(&bytes)
        .map_err(|e| bad(format!("{} is not a definitions file: {e}", file.display())))
}

fn source_of(ctx: &Ctx, input: Add) -> Result<DefsSource, CommandError> {
    let (path, store) = match input {
        Add::Embedded { defs } => return Ok(DefsSource::Embedded { defs }),
        Add::File { path, store } => (path, store),
    };
    let given = Path::new(&path);
    let absolute = if given.is_absolute() {
        given.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| CommandError::Io {
                message: e.to_string(),
            })?
            .join(given)
    };
    let file = std::fs::canonicalize(&absolute)
        .map_err(|_| bad(format!("{} not found", absolute.display())))?;
    let defs = parse_file(&file)?;
    let scene_dir = ctx
        .scene_dir()
        .map(|d| std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf()));
    let store = store.unwrap_or(if scene_dir.is_some() {
        Store::Relative
    } else {
        Store::Absolute
    });
    Ok(match store {
        Store::Absolute => DefsSource::Path {
            path: file.display().to_string(),
        },
        Store::Relative => {
            let dir = scene_dir.ok_or_else(|| {
                bad("a relative path needs the scene to be saved first")
                    .with_hint("file.save the scene, or use store absolute or embed")
            })?;
            let relative = texture::relative_to(&file, &dir).ok_or_else(|| {
                bad("the file is on another drive than the scene")
                    .with_hint("use store absolute or embed")
            })?;
            DefsSource::Path { path: relative }
        }
        Store::Embed => DefsSource::Embedded { defs },
    })
}

pub fn add(ctx: &mut Ctx, args: Add) -> CmdResult<results::Created> {
    let source = source_of(ctx, args)?;
    let id = ComponentDefsId(ctx.alloc_id());
    ctx.insert(Object::ComponentDefs(ComponentDefs {
        id,
        source,
        extra: Extra::new(),
    }));
    let catalog = Catalog::resolve(ctx.doc(), ctx.scene_dir());
    let warnings = catalog
        .problems()
        .iter()
        .filter(|p| p.from() == id)
        .map(|p| p.to_string())
        .collect();
    Ok(Outcome::with_warnings(
        results::Created::new(vec![id.into()]),
        warnings,
    ))
}

fn defs_of(ctx: &Ctx, id: ObjectId) -> Result<ComponentDefs, CommandError> {
    match id {
        ObjectId::ComponentDefs(d) => ctx
            .doc()
            .component_defs
            .get(&d)
            .cloned()
            .ok_or_else(|| CommandError::not_found(id)),
        _ if ctx.doc().contains(id) => Err(CommandError::wrong_kind(id, "entity definitions")),
        _ => Err(CommandError::not_found(id)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Remove {
    pub ids: Vec<ObjectId>,
}

pub fn remove(ctx: &mut Ctx, args: Remove) -> CmdResult<results::Deleted> {
    non_empty("ids", &args.ids)?;
    for &id in &args.ids {
        defs_of(ctx, id)?;
    }
    for &id in &args.ids {
        ctx.remove(id);
    }
    Ok(Outcome::new(results::Deleted {
        deleted: args.ids.len(),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Embed {
    /// Empty means every file-backed definitions record.
    #[serde(default)]
    pub ids: Vec<ObjectId>,
}

pub fn embed(ctx: &mut Ctx, args: Embed) -> CmdResult<results::Updated> {
    let targets: Vec<ComponentDefs> = if args.ids.is_empty() {
        ctx.doc().component_defs.values().cloned().collect()
    } else {
        args.ids
            .iter()
            .map(|&id| defs_of(ctx, id))
            .collect::<Result<_, _>>()?
    };
    let mut updated = 0;
    for mut defs in targets {
        let DefsSource::Path { path } = &defs.source else {
            continue;
        };
        let file = texture::resolve_path(path, ctx.scene_dir()).map_err(|e| bad(e.to_string()))?;
        defs.source = DefsSource::Embedded {
            defs: parse_file(&file)?,
        };
        ctx.insert(Object::ComponentDefs(defs));
        updated += 1;
    }
    Ok(Outcome::new(results::Updated { updated }))
}

pub fn list(ctx: &Ctx, _: NoArgs) -> CmdResult<results::ComponentDefsList> {
    let catalog = Catalog::resolve(ctx.doc(), ctx.scene_dir());
    let components: Vec<ComponentInfo> = catalog
        .components()
        .map(|r| ComponentInfo {
            from: r.from,
            def: r.def.clone(),
        })
        .collect();
    let defs = ctx
        .doc()
        .component_defs
        .values()
        .map(|d| DefsRecordInfo {
            id: d.id,
            source: match &d.source {
                DefsSource::Path { .. } => DefsSourceKind::Path,
                DefsSource::Embedded { .. } => DefsSourceKind::Embedded,
            },
            path: match &d.source {
                DefsSource::Path { path } => Some(path.clone()),
                DefsSource::Embedded { .. } => None,
            },
            components: components.iter().filter(|c| c.from == Some(d.id)).count(),
        })
        .collect();
    let problems = catalog
        .problems()
        .iter()
        .map(|p| DefsProblemView {
            from: p.from(),
            kind: p.kind(),
            message: p.to_string(),
        })
        .collect();
    Ok(Outcome::new(results::ComponentDefsList {
        defs,
        components,
        problems,
    }))
}
