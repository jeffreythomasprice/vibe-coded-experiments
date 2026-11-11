use std::fmt::Write;

use editor_core::{
    DVec2, DVec3, Document, Extra, FaceAttrs, OrthoView, Projection, Properties, PropertyValue,
};

use crate::{FORMAT, VERSION};

pub fn write(doc: &Document) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "format = \"{FORMAT}\"\nversion = {VERSION}");

    out.push_str("\n[meta]\n");
    let _ = writeln!(out, "name = {}", quote(&doc.name));
    let _ = writeln!(out, "next_id = {}", doc.next_id());
    write_extra(&mut out, &doc.meta_extra);

    out.push_str("\n[world]\n");
    for (k, v) in &doc.world {
        let _ = writeln!(out, "{} = {}", key(k), property(v));
    }

    out.push_str("\n[editor]\n");
    let _ = writeln!(out, "grid = {}", num(doc.editor.grid));
    write_extra(&mut out, &doc.editor.extra);

    for l in doc.layers.values() {
        out.push_str("\n[[layer]]\n");
        let _ = writeln!(out, "id = \"{}\"", l.id);
        let _ = writeln!(out, "name = {}", quote(&l.name));
        write_extra(&mut out, &l.extra);
    }

    for g in doc.groups.values() {
        out.push_str("\n[[group]]\n");
        let _ = writeln!(out, "id = \"{}\"", g.id);
        let _ = writeln!(out, "name = {}", quote(&g.name));
        let _ = writeln!(out, "layer = \"{}\"", g.layer);
        if let Some(p) = g.parent {
            let _ = writeln!(out, "group = \"{p}\"");
        }
        write_extra(&mut out, &g.extra);
    }

    for c in doc.cameras.values() {
        out.push_str("\n[[camera]]\n");
        let _ = writeln!(out, "id = \"{}\"", c.id);
        let _ = writeln!(out, "name = {}", quote(&c.name));
        let _ = writeln!(out, "position = {}", vec3(c.position));
        if c.yaw != 0.0 {
            let _ = writeln!(out, "yaw = {}", num(c.yaw));
        }
        if c.pitch != 0.0 {
            let _ = writeln!(out, "pitch = {}", num(c.pitch));
        }
        match c.projection {
            Projection::Perspective { fov } => {
                let _ = writeln!(out, "fov = {}", num(fov));
            }
            Projection::Orthographic { view, zoom } => {
                let view = match view {
                    OrthoView::Top => "top",
                    OrthoView::Front => "front",
                    OrthoView::Side => "side",
                };
                let _ = writeln!(out, "ortho = \"{view}\"");
                let _ = writeln!(out, "zoom = {}", num(zoom));
            }
        }
        write_extra(&mut out, &c.extra);
    }

    for e in doc.entities.values() {
        out.push_str("\n[[entity]]\n");
        let _ = writeln!(out, "id = \"{}\"", e.id);
        let _ = writeln!(out, "classname = {}", quote(&e.classname));
        let _ = writeln!(out, "layer = \"{}\"", e.layer);
        if let Some(g) = e.group {
            let _ = writeln!(out, "group = \"{g}\"");
        }
        if let Some(o) = e.origin {
            let _ = writeln!(out, "origin = {}", vec3(o));
        }
        if e.angles != DVec3::ZERO {
            let _ = writeln!(out, "angles = {}", vec3(e.angles));
        }
        if !e.properties.is_empty() {
            let _ = writeln!(out, "properties = {}", inline_properties(&e.properties));
        }
        write_extra(&mut out, &e.extra);
    }

    for b in doc.brushes.values() {
        out.push_str("\n[[brush]]\n");
        let _ = writeln!(out, "id = \"{}\"", b.id);
        let _ = writeln!(out, "layer = \"{}\"", b.layer);
        if let Some(g) = b.group {
            let _ = writeln!(out, "group = \"{g}\"");
        }
        if let Some(e) = b.entity {
            let _ = writeln!(out, "entity = \"{e}\"");
        }
        out.push_str("faces = [\n");
        for f in &b.brush.faces {
            let _ = writeln!(
                out,
                "  {{ id = {}, plane = [{}, {}, {}]{} }},",
                f.id.0,
                vec3(f.points[0]),
                vec3(f.points[1]),
                vec3(f.points[2]),
                attrs(&f.attrs)
            );
        }
        out.push_str("]\n");
        write_extra(&mut out, &b.extra);
    }

    for m in doc.meshes.values() {
        out.push_str("\n[[mesh]]\n");
        let _ = writeln!(out, "id = \"{}\"", m.id);
        let _ = writeln!(out, "layer = \"{}\"", m.layer);
        if let Some(g) = m.group {
            let _ = writeln!(out, "group = \"{g}\"");
        }
        if let Some(e) = m.entity {
            let _ = writeln!(out, "entity = \"{e}\"");
        }
        out.push_str("vertices = [\n");
        for v in &m.mesh.vertices {
            let _ = writeln!(out, "  {{ id = {}, p = {} }},", v.id.0, vec3(v.pos));
        }
        out.push_str("]\nfaces = [\n");
        for f in &m.mesh.faces {
            let verts: Vec<String> = f.verts.iter().map(|v| v.0.to_string()).collect();
            let _ = writeln!(
                out,
                "  {{ id = {}, verts = [{}]{} }},",
                f.id.0,
                verts.join(", "),
                attrs(&f.attrs)
            );
        }
        out.push_str("]\n");
        write_extra(&mut out, &m.extra);
    }

    out
}

fn attrs(a: &FaceAttrs) -> String {
    let mut s = format!(", material = {}", quote(&a.material));
    if a.offset != DVec2::ZERO {
        let _ = write!(s, ", offset = {}", vec2(a.offset));
    }
    if a.scale != DVec2::ONE {
        let _ = write!(s, ", scale = {}", vec2(a.scale));
    }
    if a.rotation != 0.0 {
        let _ = write!(s, ", rotation = {}", num(a.rotation));
    }
    s
}

fn write_extra(out: &mut String, extra: &Extra) {
    for (k, v) in extra {
        let _ = writeln!(out, "{} = {}", key(k), value(v));
    }
}

fn inline_properties(p: &Properties) -> String {
    let items: Vec<String> = p
        .iter()
        .map(|(k, v)| format!("{} = {}", key(k), property(v)))
        .collect();
    format!("{{ {} }}", items.join(", "))
}

fn property(v: &PropertyValue) -> String {
    match v {
        PropertyValue::Bool(b) => b.to_string(),
        PropertyValue::Number(n) => num(*n),
        PropertyValue::Vec3(v) => vec3(*v),
        PropertyValue::String(s) => quote(s),
    }
}

fn value(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => quote(s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => num(*f),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(a) => {
            let items: Vec<String> = a.iter().map(value).collect();
            format!("[{}]", items.join(", "))
        }
        toml::Value::Table(t) => {
            if t.is_empty() {
                return "{}".into();
            }
            let items: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{} = {}", key(k), value(v)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
    }
}

fn vec3(v: DVec3) -> String {
    format!("[{}, {}, {}]", num(v.x), num(v.y), num(v.z))
}

fn vec2(v: DVec2) -> String {
    format!("[{}, {}]", num(v.x), num(v.y))
}

pub(crate) fn num(x: f64) -> String {
    if x.is_nan() {
        "nan".into()
    } else if x.is_infinite() {
        if x > 0.0 { "inf".into() } else { "-inf".into() }
    } else if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        (x as i64).to_string()
    } else {
        format!("{x:?}")
    }
}

fn key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_string()
    } else {
        quote(k)
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
