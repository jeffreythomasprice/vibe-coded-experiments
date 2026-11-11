use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use editor_core::{
    Base64Bytes, Document, Filter, ImageFormat, Material, MaterialId, Object, ObjectId,
    TextureSource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results::{self, MaterialInfo, TextureInfo, TextureKind};
use super::util::non_empty;
use super::{CmdResult, NoArgs, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;
use crate::texture::{self, TextureError};

/// Largest image file a material may embed or reference.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// How a texture file is recorded in the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    /// The absolute path of the file.
    Absolute,
    /// A path relative to the directory of the scene file; the scene must be saved.
    Relative,
    /// The file's bytes, base64-encoded inside the scene.
    Embed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextureInput {
    /// An image file on disk. Relative `path`s resolve against the working directory of the client.
    File {
        path: String,
        /// How to record it; `relative` when the scene is saved, otherwise `absolute`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        store: Option<Store>,
    },
    /// The bytes of a png, jpeg, tga or bmp file, base64-encoded.
    Base64 { format: ImageFormat, data: String },
}

fn texture_error(e: TextureError) -> CommandError {
    CommandError::InvalidArgument {
        field: Some("texture".into()),
        message: e.to_string(),
        hint: Some("textures are png, jpeg, tga or bmp images".into()),
    }
}

fn canonical_dir(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
}

fn build_source(ctx: &Ctx, input: &TextureInput) -> Result<TextureSource, CommandError> {
    let source = match input {
        TextureInput::Base64 { format, data } => {
            let clean: String = data.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(clean)
                .map_err(|e| CommandError::invalid_arg("texture", format!("bad base64: {e}")))?;
            TextureSource::Embedded {
                format: *format,
                data: Base64Bytes(bytes),
            }
        }
        TextureInput::File { path, store } => {
            let given = Path::new(path);
            let absolute = if given.is_absolute() {
                given.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|e| CommandError::Io {
                        message: e.to_string(),
                    })?
                    .join(given)
            };
            let file = std::fs::canonicalize(&absolute).map_err(|_| {
                texture_error(TextureError::NotFound {
                    path: absolute.display().to_string(),
                })
            })?;
            let scene_dir = ctx.scene_dir().map(canonical_dir);
            let store = store.unwrap_or(if scene_dir.is_some() {
                Store::Relative
            } else {
                Store::Absolute
            });
            match store {
                Store::Absolute => TextureSource::Path {
                    path: file.display().to_string(),
                },
                Store::Relative => {
                    let dir = scene_dir.ok_or_else(|| {
                        CommandError::invalid_arg(
                            "texture",
                            "a relative texture path needs the scene to be saved first",
                        )
                        .with_hint("file.save the scene, or use store absolute or embed")
                    })?;
                    let relative = texture::relative_to(&file, &dir).ok_or_else(|| {
                        CommandError::invalid_arg(
                            "texture",
                            "the file is on another drive than the scene",
                        )
                        .with_hint("use store absolute or embed")
                    })?;
                    TextureSource::Path { path: relative }
                }
                Store::Embed => {
                    let format = texture::format_of_path(&file).ok_or_else(|| {
                        texture_error(TextureError::UnsupportedFormat {
                            path: file.display().to_string(),
                        })
                    })?;
                    let bytes = std::fs::read(&file).map_err(|e| CommandError::Io {
                        message: format!("{}: {e}", file.display()),
                    })?;
                    TextureSource::Embedded {
                        format,
                        data: Base64Bytes(bytes),
                    }
                }
            }
        }
    };
    check_image(ctx, &source)?;
    Ok(source)
}

fn check_image(ctx: &Ctx, source: &TextureSource) -> Result<(), CommandError> {
    let loaded = texture::load(source, ctx.scene_dir()).map_err(texture_error)?;
    if loaded.bytes.len() > MAX_IMAGE_BYTES {
        return Err(CommandError::invalid_arg(
            "texture",
            format!("the image is larger than {} MiB", MAX_IMAGE_BYTES >> 20),
        ));
    }
    texture::size_of(&loaded.bytes, loaded.format).map_err(texture_error)?;
    Ok(())
}

pub fn texture_info(source: &TextureSource, scene_dir: Option<&Path>) -> TextureInfo {
    let loaded = texture::load(source, scene_dir);
    let (kind, path) = match source {
        TextureSource::Path { path } => (TextureKind::Path, Some(path.clone())),
        TextureSource::Embedded { .. } => (TextureKind::Embedded, None),
    };
    let resolved = match source {
        TextureSource::Path { path } => texture::resolve_path(path, scene_dir)
            .ok()
            .map(|p| p.display().to_string()),
        TextureSource::Embedded { .. } => None,
    };
    let mut info = TextureInfo {
        kind,
        path,
        resolved,
        format: None,
        bytes: None,
        width: None,
        height: None,
        error: None,
    };
    match loaded {
        Err(e) => info.error = Some(e.to_string()),
        Ok(l) => {
            info.format = Some(l.format);
            info.bytes = Some(l.bytes.len());
            match texture::size_of(&l.bytes, l.format) {
                Ok(size) => {
                    info.width = Some(size.width);
                    info.height = Some(size.height);
                }
                Err(e) => info.error = Some(e.to_string()),
            }
        }
    }
    info
}

pub fn face_counts(doc: &Document) -> BTreeMap<&str, usize> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for b in doc.brushes.values() {
        for f in &b.brush.faces {
            *counts.entry(f.attrs.material.as_str()).or_default() += 1;
        }
    }
    for m in doc.meshes.values() {
        for f in &m.mesh.faces {
            *counts.entry(f.attrs.material.as_str()).or_default() += 1;
        }
    }
    counts
}

fn info(m: &Material, scene_dir: Option<&Path>, faces: usize) -> MaterialInfo {
    MaterialInfo {
        id: m.id,
        name: m.name.clone(),
        texture: m.texture.as_ref().map(|t| texture_info(t, scene_dir)),
        color: m.color,
        emissive: m.emissive,
        emissive_strength: m.emissive_strength,
        roughness: m.roughness,
        metallic: m.metallic,
        filter: m.filter,
        double_sided: m.double_sided,
        unlit: m.unlit,
        nodraw: m.nodraw,
        faces,
    }
}

/// A material by name (`base/brick`) or id (`t5`).
pub fn find<'a>(doc: &'a Document, key: &str) -> Result<&'a Material, CommandError> {
    if let Ok(id) = key.parse::<MaterialId>()
        && let Some(m) = doc.materials.get(&id)
    {
        return Ok(m);
    }
    doc.material_by_name(key)
        .ok_or_else(|| CommandError::not_found(key))
}

pub fn list(ctx: &mut Ctx, _: NoArgs) -> CmdResult<results::MaterialList> {
    let doc = ctx.doc();
    let counts = face_counts(doc);
    let materials = doc
        .materials
        .values()
        .map(|m| {
            info(
                m,
                ctx.scene_dir(),
                counts.get(m.name.as_str()).copied().unwrap_or(0),
            )
        })
        .collect();
    let undefined = counts
        .keys()
        .filter(|name| !name.is_empty() && doc.material_by_name(name).is_none())
        .map(|n| n.to_string())
        .collect();
    Ok(Outcome::new(results::MaterialList {
        materials,
        undefined,
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Get {
    /// Name or id.
    pub material: String,
}

pub fn get(ctx: &mut Ctx, args: Get) -> CmdResult<results::MaterialInfo> {
    let doc = ctx.doc();
    let m = find(doc, &args.material)?;
    let faces = face_counts(doc).get(m.name.as_str()).copied().unwrap_or(0);
    Ok(Outcome::new(info(m, ctx.scene_dir(), faces)))
}

macro_rules! material_args {
    ($(#[$meta:meta])* $name:ident { $($head:tt)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $($head)*
            /// RGBA multiplier on the image, or the flat colour without one; 0..1.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub color: Option<[f32; 4]>,
            /// RGB light the surface gives off regardless of lighting; 0..1.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub emissive: Option<[f32; 3]>,
            /// Multiplier on `emissive`; 0 turns it off.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub emissive_strength: Option<f32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub roughness: Option<f32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub metallic: Option<f32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub filter: Option<Filter>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub double_sided: Option<bool>,
            /// Draw without the editor's shading.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub unlit: Option<bool>,
            /// A surface the game does not draw: shown translucent in the editor.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub nodraw: Option<bool>,
        }

        impl $name {
            fn apply(&self, m: &mut Material) -> Result<(), CommandError> {
                if let Some(c) = self.color {
                    unit_range("color", &c)?;
                    m.color = c;
                }
                if let Some(e) = self.emissive {
                    unit_range("emissive", &e)?;
                    m.emissive = e;
                }
                if let Some(x) = self.emissive_strength {
                    non_negative("emissive_strength", x)?;
                    m.emissive_strength = x;
                }
                if let Some(x) = self.roughness {
                    unit_range("roughness", &[x])?;
                    m.roughness = x;
                }
                if let Some(x) = self.metallic {
                    unit_range("metallic", &[x])?;
                    m.metallic = x;
                }
                if let Some(x) = self.filter {
                    m.filter = x;
                }
                if let Some(x) = self.double_sided {
                    m.double_sided = x;
                }
                if let Some(x) = self.unlit {
                    m.unlit = x;
                }
                if let Some(x) = self.nodraw {
                    m.nodraw = x;
                }
                Ok(())
            }
        }
    };
}

fn unit_range(field: &str, values: &[f32]) -> Result<(), CommandError> {
    if values.iter().all(|v| (0.0..=1.0).contains(v)) {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            field,
            "values must be between 0 and 1",
        ))
    }
}

fn non_negative(field: &str, x: f32) -> Result<(), CommandError> {
    if x.is_finite() && x >= 0.0 {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            field,
            "value must be a number of at least 0",
        ))
    }
}

material_args! {
    Create {
        /// Unique in the document; faces refer to the material by this name, e.g. `base/brick`.
        pub name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub texture: Option<TextureInput>,
    }
}

material_args! {
    Set {
        /// Name or id.
        pub material: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub texture: Option<TextureInput>,
        /// Remove the image, leaving the flat colour.
        #[serde(default)]
        pub clear_texture: bool,
    }
}

fn check_name(doc: &Document, name: &str, except: Option<MaterialId>) -> Result<(), CommandError> {
    if name.trim().is_empty() {
        return Err(CommandError::invalid_arg(
            "name",
            "a material name cannot be empty",
        ));
    }
    if doc
        .materials
        .values()
        .any(|m| m.name == name && Some(m.id) != except)
    {
        return Err(CommandError::invalid_arg(
            "name",
            format!("a material called {name:?} already exists"),
        )
        .with_hint("material.set edits it, or pick another name"));
    }
    Ok(())
}

pub fn create(ctx: &mut Ctx, args: Create) -> CmdResult<results::Created> {
    check_name(ctx.doc(), &args.name, None)?;
    let id = MaterialId(ctx.alloc_id());
    let mut m = Material::new(id, args.name.clone());
    if let Some(t) = &args.texture {
        m.texture = Some(build_source(ctx, t)?);
    }
    args.apply(&mut m)?;
    ctx.insert(Object::Material(m));
    Ok(Outcome::new(results::Created::new(vec![id.into()])))
}

pub fn set(ctx: &mut Ctx, args: Set) -> CmdResult<results::IdOnly> {
    let mut m = find(ctx.doc(), &args.material)?.clone();
    if args.texture.is_some() && args.clear_texture {
        return Err(CommandError::invalid_arg(
            "clear_texture",
            "give texture or clear_texture, not both",
        ));
    }
    if let Some(t) = &args.texture {
        m.texture = Some(build_source(ctx, t)?);
    }
    if args.clear_texture {
        m.texture = None;
    }
    args.apply(&mut m)?;
    let id = m.id;
    ctx.insert(Object::Material(m));
    Ok(Outcome::new(results::IdOnly { id: id.into() }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    /// Name or id.
    pub material: String,
    pub name: String,
}

fn retarget_faces(ctx: &mut Ctx, from: &str, to: &str) -> usize {
    let mut changed = Vec::new();
    let mut faces = 0;
    for b in ctx.doc().brushes.values() {
        if b.brush.faces.iter().any(|f| f.attrs.material == from) {
            let mut b = b.clone();
            for f in &mut b.brush.faces {
                if f.attrs.material == from {
                    f.attrs.material = to.to_string();
                    faces += 1;
                }
            }
            changed.push(Object::Brush(b));
        }
    }
    for m in ctx.doc().meshes.values() {
        if m.mesh.faces.iter().any(|f| f.attrs.material == from) {
            let mut m = m.clone();
            for f in &mut m.mesh.faces {
                if f.attrs.material == from {
                    f.attrs.material = to.to_string();
                    faces += 1;
                }
            }
            changed.push(Object::Mesh(m));
        }
    }
    for o in changed {
        ctx.insert(o);
    }
    faces
}

pub fn rename(ctx: &mut Ctx, args: Rename) -> CmdResult<results::FacesChanged> {
    let mut m = find(ctx.doc(), &args.material)?.clone();
    check_name(ctx.doc(), &args.name, Some(m.id))?;
    let old = std::mem::replace(&mut m.name, args.name.clone());
    ctx.insert(Object::Material(m));
    let faces = retarget_faces(ctx, &old, &args.name);
    Ok(Outcome::new(results::FacesChanged { faces }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Embed {
    /// Names or ids; empty means every material with a file texture.
    #[serde(default)]
    pub materials: Vec<String>,
}

pub fn embed(ctx: &mut Ctx, args: Embed) -> CmdResult<results::Updated> {
    let targets: Vec<Material> = if args.materials.is_empty() {
        ctx.doc().materials.values().cloned().collect()
    } else {
        args.materials
            .iter()
            .map(|k| find(ctx.doc(), k).cloned())
            .collect::<Result<_, _>>()?
    };
    let mut updated = 0;
    for mut m in targets {
        let Some(source @ TextureSource::Path { .. }) = &m.texture else {
            continue;
        };
        let loaded = texture::load(source, ctx.scene_dir()).map_err(texture_error)?;
        m.texture = Some(TextureSource::Embedded {
            format: loaded.format,
            data: Base64Bytes(loaded.bytes),
        });
        ctx.insert(Object::Material(m));
        updated += 1;
    }
    Ok(Outcome::new(results::Updated { updated }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    /// Names or ids.
    pub materials: Vec<String>,
    /// Faces using a deleted material switch to this one (a name, which need not be defined); otherwise they keep the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_with: Option<String>,
}

pub fn delete(ctx: &mut Ctx, args: Delete) -> CmdResult<results::Deleted> {
    non_empty("materials", &args.materials)?;
    let targets: Vec<Material> = args
        .materials
        .iter()
        .map(|k| find(ctx.doc(), k).cloned())
        .collect::<Result<_, _>>()?;
    for m in &targets {
        if let Some(to) = &args.replace_with {
            retarget_faces(ctx, &m.name, to);
        }
        ctx.remove(ObjectId::Material(m.id));
    }
    Ok(Outcome::new(results::Deleted {
        deleted: targets.len(),
    }))
}
