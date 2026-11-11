use std::str::FromStr;

use base64::Engine;
use editor_core::{
    Brush, BrushFace, BrushId, BrushObject, Camera, CameraId, DVec2, DVec3, Document, Entity,
    EntityId, Extra, FaceAttrs, FaceId, Flags, Group, GroupId, HierarchyError, Layer, LayerId,
    Material, MaterialId, Mesh, MeshFace, MeshId, MeshObject, MeshVertex, ObjectId, OrthoView,
    Projection, Properties, PropertyValue, TextureSource, UvAxes, VertexId,
};
use editor_geom::brush_geometry;
use editor_geom::mesh::{Severity, validate};
use toml::{Table, Value};

use crate::{FORMAT, LoadError, VERSION, migrate};

type Result<T> = std::result::Result<T, LoadError>;

fn parse_err(ctx: &str, msg: impl std::fmt::Display) -> LoadError {
    LoadError::Parse {
        message: format!("{ctx}: {msg}"),
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn as_vec3(v: &Value) -> Option<DVec3> {
    match v {
        Value::Array(a) if a.len() == 3 => {
            Some(DVec3::new(as_f64(&a[0])?, as_f64(&a[1])?, as_f64(&a[2])?))
        }
        _ => None,
    }
}

fn as_vec2(v: &Value) -> Option<DVec2> {
    match v {
        Value::Array(a) if a.len() == 2 => Some(DVec2::new(as_f64(&a[0])?, as_f64(&a[1])?)),
        _ => None,
    }
}

fn as_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn as_u32(v: &Value) -> Option<u32> {
    v.as_integer().and_then(|i| u32::try_from(i).ok())
}

fn as_id<T: FromStr>(v: &Value) -> Option<T> {
    v.as_str().and_then(|s| s.parse().ok())
}

fn opt<T>(
    t: &mut Table,
    key: &str,
    ctx: &str,
    what: &str,
    conv: impl Fn(&Value) -> Option<T>,
) -> Result<Option<T>> {
    match t.remove(key) {
        None => Ok(None),
        Some(v) => conv(&v)
            .map(Some)
            .ok_or_else(|| parse_err(ctx, format!("`{key}` must be {what}"))),
    }
}

fn as_bool(v: &Value) -> Option<bool> {
    v.as_bool()
}

fn as_f32s<const N: usize>(v: &Value) -> Option<[f32; N]> {
    let Value::Array(a) = v else { return None };
    if a.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, x) in out.iter_mut().zip(a) {
        *slot = as_f64(x)? as f32;
    }
    Some(out)
}

fn as_f32(v: &Value) -> Option<f32> {
    as_f64(v).map(|x| x as f32)
}

fn texture(t: &mut Table, ctx: &str) -> Result<Option<TextureSource>> {
    let Some(v) = t.remove("texture") else {
        return Ok(None);
    };
    let Value::Table(mut tt) = v else {
        return Err(parse_err(ctx, "`texture` must be an inline table"));
    };
    let path = opt(&mut tt, "path", ctx, "a string", as_string)?;
    let source = match path {
        Some(path) => TextureSource::Path { path },
        None => {
            let format = req(
                &mut tt,
                "format",
                ctx,
                "\"png\", \"jpeg\", \"tga\" or \"bmp\"",
                |v| editor_core::ImageFormat::from_extension(v.as_str()?),
            )?;
            let data = req(&mut tt, "base64", ctx, "a base64 string", |v| {
                base64::engine::general_purpose::STANDARD
                    .decode(v.as_str()?)
                    .ok()
                    .map(editor_core::Base64Bytes)
            })?;
            TextureSource::Embedded { format, data }
        }
    };
    no_leftovers(&tt, ctx)?;
    Ok(Some(source))
}

fn flags(t: &mut Table, ctx: &str) -> Result<Flags> {
    Ok(Flags {
        hidden: opt(t, "hidden", ctx, "a boolean", as_bool)?.unwrap_or(false),
        locked: opt(t, "locked", ctx, "a boolean", as_bool)?.unwrap_or(false),
    })
}

fn req<T>(
    t: &mut Table,
    key: &str,
    ctx: &str,
    what: &str,
    conv: impl Fn(&Value) -> Option<T>,
) -> Result<T> {
    opt(t, key, ctx, what, conv)?.ok_or_else(|| parse_err(ctx, format!("missing `{key}`")))
}

fn rest(t: Table) -> Extra {
    t.into_iter().collect()
}

fn records(root: &mut Table, key: &str) -> Result<Vec<Table>> {
    match root.remove(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .into_iter()
            .map(|v| match v {
                Value::Table(t) => Ok(t),
                _ => Err(parse_err(key, "expected an array of tables")),
            })
            .collect(),
        Some(_) => Err(parse_err(key, "expected an array of tables")),
    }
}

fn sub_table(root: &mut Table, key: &str) -> Result<Table> {
    match root.remove(key) {
        None => Ok(Table::new()),
        Some(Value::Table(t)) => Ok(t),
        Some(_) => Err(parse_err(key, "expected a table")),
    }
}

fn properties(t: Table, ctx: &str) -> Result<Properties> {
    let mut out = Properties::new();
    for (k, v) in t {
        let pv = match &v {
            Value::Boolean(b) => PropertyValue::Bool(*b),
            Value::Integer(_) | Value::Float(_) => PropertyValue::Number(as_f64(&v).unwrap()),
            Value::String(s) => PropertyValue::String(s.clone()),
            Value::Array(_) => match as_vec3(&v) {
                Some(v3) => PropertyValue::Vec3(v3),
                None => return Err(parse_err(ctx, format!("property `{k}` must be a vec3"))),
            },
            _ => {
                return Err(parse_err(
                    ctx,
                    format!("property `{k}` has an unsupported type"),
                ));
            }
        };
        out.insert(k, pv);
    }
    Ok(out)
}

fn face_attrs(t: &mut Table, ctx: &str) -> Result<FaceAttrs> {
    let defaults = FaceAttrs::default();
    let u_axis = opt(t, "u_axis", ctx, "an [x, y, z] vector", as_vec3)?;
    let v_axis = opt(t, "v_axis", ctx, "an [x, y, z] vector", as_vec3)?;
    let axes = match (u_axis, v_axis) {
        (Some(u), Some(v)) => Some(UvAxes { u, v }),
        (None, None) => None,
        _ => return Err(parse_err(ctx, "`u_axis` and `v_axis` go together")),
    };
    Ok(FaceAttrs {
        axes,
        material: opt(t, "material", ctx, "a string", as_string)?.unwrap_or_default(),
        offset: opt(t, "offset", ctx, "a [x, y] pair", as_vec2)?.unwrap_or(defaults.offset),
        scale: opt(t, "scale", ctx, "a [x, y] pair", as_vec2)?.unwrap_or(defaults.scale),
        rotation: opt(t, "rotation", ctx, "a number", as_f64)?.unwrap_or(defaults.rotation),
    })
}

fn face_tables(t: &mut Table, key: &str, ctx: &str) -> Result<Vec<Table>> {
    match t.remove(key) {
        None => Err(parse_err(ctx, format!("missing `{key}`"))),
        Some(Value::Array(items)) => items
            .into_iter()
            .map(|v| match v {
                Value::Table(t) => Ok(t),
                _ => Err(parse_err(
                    ctx,
                    format!("`{key}` entries must be inline tables"),
                )),
            })
            .collect(),
        Some(_) => Err(parse_err(ctx, format!("`{key}` must be an array"))),
    }
}

fn no_leftovers(t: &Table, ctx: &str) -> Result<()> {
    match t.keys().next() {
        None => Ok(()),
        Some(k) => Err(parse_err(ctx, format!("unknown key `{k}`"))),
    }
}

fn parse_brush_faces(t: &mut Table, ctx: &str) -> Result<Brush> {
    let mut faces = Vec::new();
    for mut ft in face_tables(t, "faces", ctx)? {
        let id = FaceId(req(&mut ft, "id", ctx, "a face id", as_u32)?);
        let fctx = format!("{ctx}/f{}", id.0);
        let plane = match ft.remove("plane") {
            Some(Value::Array(a)) if a.len() == 3 => {
                let pts: Option<Vec<DVec3>> = a.iter().map(as_vec3).collect();
                pts.ok_or_else(|| parse_err(&fctx, "`plane` must be three [x, y, z] points"))?
            }
            _ => return Err(parse_err(&fctx, "`plane` must be three [x, y, z] points")),
        };
        let attrs = face_attrs(&mut ft, &fctx)?;
        no_leftovers(&ft, &fctx)?;
        if faces.iter().any(|f: &BrushFace| f.id == id) {
            return Err(LoadError::DuplicateId { id: fctx });
        }
        faces.push(BrushFace {
            id,
            points: [plane[0], plane[1], plane[2]],
            attrs,
        });
    }
    Ok(Brush { faces })
}

fn parse_mesh(t: &mut Table, ctx: &str) -> Result<Mesh> {
    let mut vertices: Vec<MeshVertex> = Vec::new();
    for mut vt in face_tables(t, "vertices", ctx)? {
        let id = VertexId(req(&mut vt, "id", ctx, "a vertex id", as_u32)?);
        let vctx = format!("{ctx}/v{}", id.0);
        let pos = req(&mut vt, "p", &vctx, "an [x, y, z] point", as_vec3)?;
        no_leftovers(&vt, &vctx)?;
        if vertices.iter().any(|v| v.id == id) {
            return Err(LoadError::DuplicateId { id: vctx });
        }
        vertices.push(MeshVertex { id, pos });
    }
    let mut faces: Vec<MeshFace> = Vec::new();
    for mut ft in face_tables(t, "faces", ctx)? {
        let id = FaceId(req(&mut ft, "id", ctx, "a face id", as_u32)?);
        let fctx = format!("{ctx}/f{}", id.0);
        let verts = req(
            &mut ft,
            "verts",
            &fctx,
            "an array of vertex ids",
            |v| match v {
                Value::Array(a) => a.iter().map(|x| as_u32(x).map(VertexId)).collect(),
                _ => None,
            },
        )?;
        let attrs = face_attrs(&mut ft, &fctx)?;
        no_leftovers(&ft, &fctx)?;
        if faces.iter().any(|f| f.id == id) {
            return Err(LoadError::DuplicateId { id: fctx });
        }
        faces.push(MeshFace { id, verts, attrs });
    }
    Ok(Mesh { vertices, faces })
}

fn insert_checked(doc: &mut Document, object: editor_core::Object) -> Result<()> {
    let id = object.id();
    if doc.insert(object).is_some() {
        return Err(LoadError::DuplicateId { id: id.to_string() });
    }
    Ok(())
}

pub fn read(text: &str) -> Result<Document> {
    let mut root: Table = text
        .parse()
        .map_err(|e: toml::de::Error| LoadError::Parse {
            message: e.to_string(),
        })?;

    match root.remove("format") {
        Some(Value::String(s)) if s == FORMAT => {}
        _ => {
            return Err(parse_err(
                "header",
                format!("`format` must be \"{FORMAT}\""),
            ));
        }
    }
    let found = match root.remove("version") {
        Some(Value::Integer(i)) => i,
        _ => return Err(parse_err("header", "`version` must be an integer")),
    };
    if found > VERSION as i64 {
        return Err(LoadError::UnsupportedVersion {
            found,
            supported: VERSION,
        });
    }
    if found < 1 {
        return Err(parse_err("header", "`version` must be at least 1"));
    }
    migrate(&mut root, found as u32);

    let mut meta = sub_table(&mut root, "meta")?;
    let name = opt(&mut meta, "name", "meta", "a string", as_string)?.unwrap_or_default();
    let next_id = opt(&mut meta, "next_id", "meta", "an integer", |v| {
        v.as_integer().and_then(|i| u64::try_from(i).ok())
    })?
    .unwrap_or(1);
    let mut doc = Document::empty(name, next_id);
    doc.meta_extra = rest(meta);

    doc.world = properties(sub_table(&mut root, "world")?, "world")?;

    let mut editor = sub_table(&mut root, "editor")?;
    if let Some(grid) = opt(&mut editor, "grid", "editor", "a number", as_f64)? {
        doc.editor.grid = grid;
    }
    doc.editor.extra = rest(editor);

    for mut t in records(&mut root, "material")? {
        let id: MaterialId = req(&mut t, "id", "material", "a material id", as_id)?;
        let ctx = id.to_string();
        let name = req(&mut t, "name", &ctx, "a string", as_string)?;
        let mut m = Material::new(id, name);
        m.texture = texture(&mut t, &ctx)?;
        if let Some(c) = opt(&mut t, "color", &ctx, "an [r, g, b, a] array", as_f32s::<4>)? {
            m.color = c;
        }
        if let Some(c) = opt(&mut t, "emissive", &ctx, "an [r, g, b] array", as_f32s::<3>)? {
            m.emissive = c;
        }
        if let Some(x) = opt(&mut t, "emissive_strength", &ctx, "a number", as_f32)? {
            m.emissive_strength = x;
        }
        if let Some(x) = opt(&mut t, "roughness", &ctx, "a number", as_f32)? {
            m.roughness = x;
        }
        if let Some(x) = opt(&mut t, "metallic", &ctx, "a number", as_f32)? {
            m.metallic = x;
        }
        if let Some(f) = opt(
            &mut t,
            "filter",
            &ctx,
            "\"linear\" or \"nearest\"",
            |v| match v.as_str()? {
                "linear" => Some(editor_core::Filter::Linear),
                "nearest" => Some(editor_core::Filter::Nearest),
                _ => None,
            },
        )? {
            m.filter = f;
        }
        m.double_sided = opt(&mut t, "double_sided", &ctx, "a boolean", as_bool)?.unwrap_or(false);
        m.unlit = opt(&mut t, "unlit", &ctx, "a boolean", as_bool)?.unwrap_or(false);
        m.nodraw = opt(&mut t, "nodraw", &ctx, "a boolean", as_bool)?.unwrap_or(false);
        m.extra = rest(t);
        if let Some(first) = doc.materials.values().find(|o| o.name == m.name) {
            return Err(LoadError::DuplicateMaterialName {
                name: m.name,
                first: first.id.into(),
                second: id.into(),
            });
        }
        insert_checked(&mut doc, editor_core::Object::Material(m))?;
    }

    for mut t in records(&mut root, "layer")? {
        let id: LayerId = req(&mut t, "id", "layer", "a layer id", as_id)?;
        let ctx = id.to_string();
        let name = req(&mut t, "name", &ctx, "a string", as_string)?;
        let layer = Layer {
            flags: flags(&mut t, &ctx)?,
            omit_from_export: opt(&mut t, "omit_from_export", &ctx, "a boolean", as_bool)?
                .unwrap_or(false),
            id,
            name,
            extra: rest(t),
        };
        insert_checked(&mut doc, editor_core::Object::Layer(layer))?;
    }

    for mut t in records(&mut root, "group")? {
        let id: GroupId = req(&mut t, "id", "group", "a group id", as_id)?;
        let ctx = id.to_string();
        let group = Group {
            flags: flags(&mut t, &ctx)?,
            id,
            name: req(&mut t, "name", &ctx, "a string", as_string)?,
            layer: req(&mut t, "layer", &ctx, "a layer id", as_id)?,
            parent: opt(&mut t, "group", &ctx, "a group id", as_id)?,
            extra: Extra::new(),
        };
        let group = Group {
            extra: rest(t),
            ..group
        };
        insert_checked(&mut doc, editor_core::Object::Group(group))?;
    }

    for mut t in records(&mut root, "camera")? {
        let id: CameraId = req(&mut t, "id", "camera", "a camera id", as_id)?;
        let ctx = id.to_string();
        let name = req(&mut t, "name", &ctx, "a string", as_string)?;
        let position = req(&mut t, "position", &ctx, "an [x, y, z] point", as_vec3)?;
        let yaw = opt(&mut t, "yaw", &ctx, "a number", as_f64)?.unwrap_or(0.0);
        let pitch = opt(&mut t, "pitch", &ctx, "a number", as_f64)?.unwrap_or(0.0);
        let ortho = opt(
            &mut t,
            "ortho",
            &ctx,
            "\"top\", \"front\" or \"side\"",
            |v| match v.as_str()? {
                "top" => Some(OrthoView::Top),
                "front" => Some(OrthoView::Front),
                "side" => Some(OrthoView::Side),
                _ => None,
            },
        )?;
        let projection = match ortho {
            Some(view) => Projection::Orthographic {
                view,
                zoom: req(&mut t, "zoom", &ctx, "a number", as_f64)?,
            },
            None => Projection::Perspective {
                fov: req(&mut t, "fov", &ctx, "a number", as_f64)?,
            },
        };
        let camera = Camera {
            id,
            name,
            position,
            yaw,
            pitch,
            projection,
            extra: rest(t),
        };
        insert_checked(&mut doc, editor_core::Object::Camera(camera))?;
    }

    for mut t in records(&mut root, "entity")? {
        let id: EntityId = req(&mut t, "id", "entity", "an entity id", as_id)?;
        let ctx = id.to_string();
        let classname = req(&mut t, "classname", &ctx, "a string", as_string)?;
        let layer = req(&mut t, "layer", &ctx, "a layer id", as_id)?;
        let group = opt(&mut t, "group", &ctx, "a group id", as_id)?;
        let origin = opt(&mut t, "origin", &ctx, "an [x, y, z] point", as_vec3)?;
        let angles =
            opt(&mut t, "angles", &ctx, "an [x, y, z] triple", as_vec3)?.unwrap_or(DVec3::ZERO);
        let props = match t.remove("properties") {
            None => Properties::new(),
            Some(Value::Table(p)) => properties(p, &ctx)?,
            Some(_) => return Err(parse_err(&ctx, "`properties` must be a table")),
        };
        let entity = Entity {
            flags: flags(&mut t, &ctx)?,
            id,
            classname,
            layer,
            group,
            origin,
            angles,
            properties: props,
            extra: rest(t),
        };
        insert_checked(&mut doc, editor_core::Object::Entity(entity))?;
    }

    for mut t in records(&mut root, "brush")? {
        let id: BrushId = req(&mut t, "id", "brush", "a brush id", as_id)?;
        let ctx = id.to_string();
        let layer = req(&mut t, "layer", &ctx, "a layer id", as_id)?;
        let group = opt(&mut t, "group", &ctx, "a group id", as_id)?;
        let entity = opt(&mut t, "entity", &ctx, "an entity id", as_id)?;
        let brush = parse_brush_faces(&mut t, &ctx)?;
        let object = BrushObject {
            flags: flags(&mut t, &ctx)?,
            id,
            layer,
            group,
            entity,
            brush,
            extra: rest(t),
        };
        insert_checked(&mut doc, editor_core::Object::Brush(object))?;
    }

    for mut t in records(&mut root, "mesh")? {
        let id: MeshId = req(&mut t, "id", "mesh", "a mesh id", as_id)?;
        let ctx = id.to_string();
        let layer = req(&mut t, "layer", &ctx, "a layer id", as_id)?;
        let group = opt(&mut t, "group", &ctx, "a group id", as_id)?;
        let entity = opt(&mut t, "entity", &ctx, "an entity id", as_id)?;
        let mesh = parse_mesh(&mut t, &ctx)?;
        let object = MeshObject {
            flags: flags(&mut t, &ctx)?,
            id,
            layer,
            group,
            entity,
            mesh,
            extra: rest(t),
        };
        insert_checked(&mut doc, editor_core::Object::Mesh(object))?;
    }

    if let Some(k) = root.keys().next() {
        return Err(parse_err(
            "document",
            format!("unknown top-level key `{k}`"),
        ));
    }

    editor_core::hierarchy::check(&doc).map_err(|e| match e {
        HierarchyError::DuplicateNumber { id, .. } => LoadError::DuplicateId { id: id.to_string() },
        HierarchyError::DanglingReference { id, field, target } => {
            LoadError::DanglingReference { id, field, target }
        }
        HierarchyError::GroupCycle(id) => LoadError::GroupCycle { id },
    })?;

    for b in doc.brushes.values() {
        if let Err(e) = brush_geometry(&b.brush) {
            return Err(LoadError::InvalidGeometry {
                id: ObjectId::Brush(b.id),
                reason: e.to_string(),
            });
        }
    }
    for m in doc.meshes.values() {
        if let Some(issue) = validate(&m.mesh)
            .into_iter()
            .find(|i| i.severity() == Severity::Error)
        {
            return Err(LoadError::InvalidGeometry {
                id: ObjectId::Mesh(m.id),
                reason: format!("{issue:?}"),
            });
        }
    }

    if let Some(max) = doc.all_ids().iter().map(|i| i.number()).max()
        && doc.next_id() <= max
    {
        doc.set_next_id(max + 1);
    }

    Ok(doc)
}
