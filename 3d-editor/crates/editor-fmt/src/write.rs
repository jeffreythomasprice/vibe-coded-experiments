use std::fmt::Write;

use base64::Engine;

use editor_core::{
    ComponentDefs, DVec2, DVec3, DefsSource, Document, Extra, FaceAttrs, Filter, Flags, Material,
    OrthoView, Projection, Properties, TextureSource,
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
        let _ = writeln!(out, "{} = {}", key(k), json(v));
    }

    out.push_str("\n[editor]\n");
    let _ = writeln!(out, "grid = {}", num(doc.editor.grid));
    write_extra(&mut out, &doc.editor.extra);

    for m in doc.materials.values() {
        write_material(&mut out, m);
    }

    for d in doc.component_defs.values() {
        write_defs(&mut out, d);
    }

    for l in doc.layers.values() {
        out.push_str("\n[[layer]]\n");
        let _ = writeln!(out, "id = \"{}\"", l.id);
        let _ = writeln!(out, "name = {}", quote(&l.name));
        write_flags(&mut out, l.flags);
        if l.omit_from_export {
            out.push_str("omit_from_export = true\n");
        }
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
        write_flags(&mut out, g.flags);
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
        let _ = writeln!(out, "layer = \"{}\"", e.layer);
        if let Some(g) = e.group {
            let _ = writeln!(out, "group = \"{g}\"");
        }
        write_flags(&mut out, e.flags);
        let _ = writeln!(out, "origin = {}", vec3(e.origin));
        if e.angles != DVec3::ZERO {
            let _ = writeln!(out, "angles = {}", vec3(e.angles));
        }
        if !e.components.is_empty() {
            let _ = writeln!(out, "components = {}", inline_components(&e.components));
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
        write_flags(&mut out, b.flags);
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
        write_flags(&mut out, m.flags);
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

fn write_material(out: &mut String, m: &Material) {
    let defaults = Material::new(m.id, "");
    out.push_str("\n[[material]]\n");
    let _ = writeln!(out, "id = \"{}\"", m.id);
    let _ = writeln!(out, "name = {}", quote(&m.name));
    match &m.texture {
        None => {}
        Some(TextureSource::Path { path }) => {
            let _ = writeln!(out, "texture = {{ path = {} }}", quote(path));
        }
        Some(TextureSource::Embedded { format, data }) => {
            let text = base64::engine::general_purpose::STANDARD.encode(&data.0);
            let _ = writeln!(
                out,
                "texture = {{ format = \"{format}\", base64 = \"{text}\" }}"
            );
        }
    }
    if m.color != defaults.color {
        let _ = writeln!(out, "color = {}", floats(&m.color));
    }
    if m.emissive != defaults.emissive {
        let _ = writeln!(out, "emissive = {}", floats(&m.emissive));
    }
    if m.emissive_strength != defaults.emissive_strength {
        let _ = writeln!(out, "emissive_strength = {}", num32(m.emissive_strength));
    }
    if m.roughness != defaults.roughness {
        let _ = writeln!(out, "roughness = {}", num32(m.roughness));
    }
    if m.metallic != defaults.metallic {
        let _ = writeln!(out, "metallic = {}", num32(m.metallic));
    }
    if m.filter == Filter::Nearest {
        out.push_str("filter = \"nearest\"\n");
    }
    for (name, on) in [
        ("double_sided", m.double_sided),
        ("unlit", m.unlit),
        ("nodraw", m.nodraw),
    ] {
        if on {
            let _ = writeln!(out, "{name} = true");
        }
    }
    write_extra(out, &m.extra);
}

fn floats(v: &[f32]) -> String {
    let items: Vec<String> = v.iter().map(|x| num32(*x)).collect();
    format!("[{}]", items.join(", "))
}

fn num32(x: f32) -> String {
    if x.fract() == 0.0 && x.abs() < 16_777_216.0 {
        (x as i64).to_string()
    } else {
        format!("{x:?}")
    }
}

fn write_flags(out: &mut String, flags: Flags) {
    if flags.hidden {
        out.push_str("hidden = true\n");
    }
    if flags.locked {
        out.push_str("locked = true\n");
    }
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
    if let Some(axes) = a.axes {
        let _ = write!(s, ", u_axis = {}, v_axis = {}", vec3(axes.u), vec3(axes.v));
    }
    s
}

fn write_extra(out: &mut String, extra: &Extra) {
    for (k, v) in extra {
        let _ = writeln!(out, "{} = {}", key(k), value(v));
    }
}

fn inline_components(c: &std::collections::BTreeMap<String, Properties>) -> String {
    let map: serde_json::Map<String, serde_json::Value> = c
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::Object(v.clone())))
        .collect();
    json(&serde_json::Value::Object(map))
}

fn write_defs(out: &mut String, d: &ComponentDefs) {
    out.push_str("\n[[component_defs]]\n");
    let _ = writeln!(out, "id = \"{}\"", d.id);
    match &d.source {
        DefsSource::Path { path } => {
            let _ = writeln!(out, "path = {}", quote(path));
        }
        DefsSource::Embedded { defs } => {
            let text = serde_json::to_string_pretty(defs).expect("definitions serialize");
            if text.contains("\'\'\'") {
                let _ = writeln!(out, "json = {}", quote(&text));
            } else {
                let _ = writeln!(out, "json = \'\'\'\n{text}\n\'\'\'");
            }
        }
    }
    write_extra(out, &d.extra);
}

fn json(v: &serde_json::Value) -> String {
    use serde_json::Value as J;
    match v {
        J::Null => "\"\"".into(),
        J::Bool(b) => b.to_string(),
        J::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{f:.1}")
                } else {
                    format!("{f:?}")
                }
            }
        }
        J::String(s) => quote(s),
        J::Array(items) => {
            let items: Vec<String> = items.iter().map(json).collect();
            format!("[{}]", items.join(", "))
        }
        J::Object(map) => {
            if map.is_empty() {
                return "{}".into();
            }
            let mut entries: Vec<(&String, &J)> = map.iter().collect();
            entries.sort_by_key(|(k, _)| k.as_str());
            let items: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("{} = {}", key(k), json(v)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
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
