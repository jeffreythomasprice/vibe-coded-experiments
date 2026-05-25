//! Low-level AcroForm field plumbing on top of `lopdf`.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use super::PdfRenderError;
use super::encoding::{to_text_string, winansi_literal};

/// Index of AcroForm field name → terminal field object ID.
///
/// AcroForm fields form a tree (`/Fields` with `/Kids`), but the leaves we
/// want to write — text fields, checkboxes — are identified by their fully
/// qualified `/T` name. Field names in this template are not nested
/// (no parent/child dotted paths) so a simple flat name table suffices.
pub(super) struct FieldIndex {
    by_name: HashMap<String, ObjectId>,
    /// Resolved reference to the AcroForm's `/DR /Font /Helv` font, used
    /// when baking appearance streams for text fields. `None` if the
    /// template doesn't declare Helv (it does, in practice).
    helv_font: Option<ObjectId>,
}

impl FieldIndex {
    pub(super) fn build(doc: &Document) -> Self {
        let mut by_name = HashMap::new();
        if let Some(root_id) = acroform_fields_root(doc) {
            walk(doc, root_id, &mut by_name);
        }
        let helv_font = resolve_helv_font(doc);
        FieldIndex { by_name, helv_font }
    }

    pub(super) fn get(&self, name: &str) -> Result<ObjectId, PdfRenderError> {
        self.by_name
            .get(name)
            .copied()
            .ok_or_else(|| PdfRenderError::MissingField(name.to_string()))
    }

    /// `true` if the named field exists in the template.
    pub(super) fn has(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Iterate all known field names. Used by tests.
    #[cfg(test)]
    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(|s| s.as_str())
    }
}

fn acroform_fields_root(doc: &Document) -> Option<ObjectId> {
    let catalog = doc.catalog().ok()?;
    let form = catalog.get(b"AcroForm").ok()?;
    let form_dict = match form {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    // The /Fields entry is an array of references.
    let _fields = form_dict.get(b"Fields").ok()?;
    // We need the parent dict ID to walk; return the catalog's AcroForm
    // dictionary by re-resolving its reference, or use a sentinel.
    // For walking purposes, return a synthetic root we won't actually
    // dereference — we handle it in `walk`.
    Some((0, 0))
}

fn walk(doc: &Document, _root: ObjectId, out: &mut HashMap<String, ObjectId>) {
    // Start from /Root/AcroForm/Fields directly.
    let Ok(catalog) = doc.catalog() else { return };
    let Ok(form_obj) = catalog.get(b"AcroForm") else {
        return;
    };
    let form_dict = match form_obj {
        Object::Reference(id) => match doc.get_dictionary(*id) {
            Ok(d) => d,
            Err(_) => return,
        },
        Object::Dictionary(d) => d,
        _ => return,
    };
    let Ok(fields_obj) = form_dict.get(b"Fields") else {
        return;
    };
    let Ok(fields_arr) = fields_obj.as_array() else {
        return;
    };
    for f in fields_arr {
        if let Ok(id) = f.as_reference() {
            walk_field(doc, id, None, out);
        }
    }
}

fn walk_field(
    doc: &Document,
    id: ObjectId,
    inherited_name: Option<&str>,
    out: &mut HashMap<String, ObjectId>,
) {
    let Ok(dict) = doc.get_dictionary(id) else {
        return;
    };
    // Build qualified name.
    let local_name = dict.get(b"T").ok().and_then(|o| match o {
        Object::String(b, _) => std::str::from_utf8(b).ok().map(str::to_string),
        _ => None,
    });
    let full_name = match (inherited_name, local_name.as_deref()) {
        (None, Some(n)) => Some(n.to_string()),
        (Some(parent), Some(n)) => Some(format!("{}.{}", parent, n)),
        (Some(parent), None) => Some(parent.to_string()),
        (None, None) => None,
    };

    // If this node has /Kids, recurse.
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        // A field that has /Kids but also a /T is a parent; recursing children
        // inherit the qualified name. However, if /Kids are widget annotations
        // (no /T), this is still a terminal field with multiple widgets — record it.
        let has_child_field = kids.iter().any(|k| {
            if let Ok(kid_id) = k.as_reference() {
                doc.get_dictionary(kid_id)
                    .map(|d| d.has(b"T"))
                    .unwrap_or(false)
            } else {
                false
            }
        });
        if has_child_field {
            for k in kids {
                if let Ok(kid_id) = k.as_reference() {
                    walk_field(doc, kid_id, full_name.as_deref(), out);
                }
            }
            return;
        }
        // No child has /T → this is a terminal field with widget kids.
        if let Some(name) = full_name {
            out.insert(name, id);
        }
        return;
    }

    // Leaf field with no kids.
    if let Some(name) = full_name {
        out.insert(name, id);
    }
}

/// Resolve `/Root /AcroForm /DR /Font /Helv` to an ObjectId. Returns the
/// indirect reference target so it can be embedded in a Form XObject's
/// resource dictionary.
fn resolve_helv_font(doc: &Document) -> Option<ObjectId> {
    let catalog = doc.catalog().ok()?;
    let form_dict = match catalog.get(b"AcroForm").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let dr = match form_dict.get(b"DR").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let fonts = match dr.get(b"Font").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    fonts.get(b"Helv").ok()?.as_reference().ok()
}

/// Set the value (`/V`) of a text field and bake an appearance stream so
/// viewers that don't honor `/NeedAppearances` (pdf.js, Chrome) still
/// render the text. Choice fields (`/FT /Ch`) are mutated into plain text
/// fields first, since the sheet uses them as dropdowns whose `/Opt` list
/// is incompatible with the free-form character data we want to write.
pub(super) fn set_text_field(
    doc: &mut Document,
    index: &FieldIndex,
    name: &str,
    value: &str,
) -> Result<(), PdfRenderError> {
    let id = index.get(name)?;

    // Convert a Choice field to a plain Text field if needed, then read the
    // widgets we need to draw appearances on.
    convert_choice_to_text(doc, id)?;

    // Set /V on the field dict and clear the cached /AP that pointed at the
    // choice popup (we will write fresh per-widget appearances below).
    {
        let dict = doc
            .get_object_mut(id)
            .map_err(PdfRenderError::TemplateParse)?
            .as_dict_mut()
            .map_err(PdfRenderError::TemplateParse)?;
        dict.set(
            "V",
            Object::String(to_text_string(value), StringFormat::Hexadecimal),
        );
        dict.remove(b"AP");
    }

    // Bake a Form XObject for each widget annotation that backs this field.
    // For a terminal field that is itself a widget (has /Rect), draw on the
    // field dict; otherwise draw on each kid widget.
    if let Some(helv) = index.helv_font {
        for (widget_id, rect, da_size) in widgets_for(doc, id) {
            let size = pick_font_size(da_size, rect[3] - rect[1]);
            let ap_id = build_text_appearance(doc, rect, value, helv, size);
            if let Ok(widget) = doc.get_object_mut(widget_id) {
                if let Ok(widget_dict) = widget.as_dict_mut() {
                    let mut ap = Dictionary::new();
                    ap.set("N", Object::Reference(ap_id));
                    widget_dict.set("AP", Object::Dictionary(ap));
                }
            }
        }
    }
    Ok(())
}

/// If `id` is a Choice field (`/FT /Ch`), rewrite it as a text field so we
/// can set an arbitrary `/V` and bake our own appearance stream. The sheet
/// uses choices as dropdowns whose `/Opt` list constrains the value — that
/// constraint isn't useful for free-form character data (custom background
/// labels, charm names, etc.).
fn convert_choice_to_text(doc: &mut Document, id: ObjectId) -> Result<(), PdfRenderError> {
    let kid_ids: Vec<ObjectId> = {
        let dict = doc
            .get_object_mut(id)
            .map_err(PdfRenderError::TemplateParse)?
            .as_dict_mut()
            .map_err(PdfRenderError::TemplateParse)?;
        let is_choice = matches!(dict.get(b"FT").ok(), Some(Object::Name(n)) if n == b"Ch");
        if is_choice {
            dict.set("FT", Object::Name(b"Tx".to_vec()));
            dict.remove(b"Opt");
            dict.remove(b"I");
            dict.remove(b"TI");
        }
        dict.get(b"Kids")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default()
    };
    for kid_id in kid_ids {
        if let Ok(kid_obj) = doc.get_object_mut(kid_id) {
            if let Ok(kid_dict) = kid_obj.as_dict_mut() {
                let is_choice =
                    matches!(kid_dict.get(b"FT").ok(), Some(Object::Name(n)) if n == b"Ch");
                if is_choice {
                    kid_dict.set("FT", Object::Name(b"Tx".to_vec()));
                    kid_dict.remove(b"Opt");
                    kid_dict.remove(b"I");
                    kid_dict.remove(b"TI");
                }
            }
        }
    }
    Ok(())
}

/// Returns `(widget_id, rect)` for the first widget annotation backing the
/// named field, or `None` if the field is missing or has no widget with a
/// `/Rect`. Used by overlay drawing that needs a field's on-page geometry
/// but not its appearance-stream details.
pub(super) fn first_widget_rect(
    doc: &Document,
    index: &FieldIndex,
    name: &str,
) -> Option<(ObjectId, [f64; 4])> {
    let id = index.get(name).ok()?;
    widgets_for(doc, id)
        .into_iter()
        .next()
        .map(|(w, r, _)| (w, r))
}

/// Returns one `(widget_id, rect, da_font_size)` triple per widget
/// annotation that backs the field at `id`. If the field is itself a
/// widget (has `/Rect`), returns just that one entry. Otherwise iterates
/// the field's `/Kids`. `da_font_size` is parsed from the widget's `/DA`
/// (e.g. `/Helv 10 Tf 0 g` → `10.0`), defaulting to 0 if absent (caller
/// picks a fallback).
fn widgets_for(doc: &Document, id: ObjectId) -> Vec<(ObjectId, [f64; 4], f64)> {
    let mut out = Vec::new();
    let Ok(dict) = doc.get_dictionary(id) else {
        return out;
    };
    if let Some(rect) = read_rect(dict) {
        let size = parse_da_font_size(dict);
        out.push((id, rect, size));
        return out;
    }
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            let Ok(kid_id) = k.as_reference() else {
                continue;
            };
            let Ok(kid_dict) = doc.get_dictionary(kid_id) else {
                continue;
            };
            if let Some(rect) = read_rect(kid_dict) {
                let size = parse_da_font_size(kid_dict);
                out.push((kid_id, rect, size));
            }
        }
    }
    out
}

fn read_rect(dict: &Dictionary) -> Option<[f64; 4]> {
    let arr = dict.get(b"Rect").ok()?.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let n = |o: &Object| -> Option<f64> {
        match o {
            Object::Integer(i) => Some(*i as f64),
            Object::Real(r) => Some(*r as f64),
            _ => None,
        }
    };
    Some([n(&arr[0])?, n(&arr[1])?, n(&arr[2])?, n(&arr[3])?])
}

/// Extract the font size from a `/DA` string like `/Helv 10 Tf 0 g`.
/// Returns 0.0 if no `/DA` is set or the size token can't be parsed.
fn parse_da_font_size(dict: &Dictionary) -> f64 {
    let Ok(da) = dict.get(b"DA") else { return 0.0 };
    let bytes = match da {
        Object::String(b, _) => b.as_slice(),
        _ => return 0.0,
    };
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return 0.0,
    };
    // Find " Tf" and walk backwards over the size token.
    let Some(tf_idx) = s.find(" Tf") else {
        return 0.0;
    };
    let before = s[..tf_idx].trim_end();
    let size_token = before.split_whitespace().last().unwrap_or("");
    size_token.parse::<f64>().unwrap_or(0.0)
}

/// Choose a font size for the appearance stream, given the widget's
/// declared `/DA` size (0 = auto-fit) and the widget's pixel height.
fn pick_font_size(declared: f64, height: f64) -> f64 {
    let max_for_height = (height * 0.75).max(4.0);
    let chosen = if declared > 0.0 { declared } else { 9.0 };
    chosen.min(max_for_height).max(4.0)
}

/// Build a Form XObject containing a single line of text rendered with
/// Helv at `size` points inside the widget's bbox. Returns the new
/// object's ID; the caller wires it into the widget's `/AP /N`.
fn build_text_appearance(
    doc: &mut Document,
    rect: [f64; 4],
    value: &str,
    helv: ObjectId,
    size: f64,
) -> ObjectId {
    let w = (rect[2] - rect[0]).max(0.0);
    let h = (rect[3] - rect[1]).max(0.0);
    let baseline = ((h - size) / 2.0).max(1.0);
    let escaped = winansi_literal(value);

    let mut content: Vec<u8> = Vec::with_capacity(escaped.len() + 96);
    content.extend_from_slice(b"/Tx BMC\nq\n");
    // Clip to the bbox so overflow text is hidden rather than spilling
    // outside the field on viewers that draw the XObject unclipped.
    content.extend_from_slice(format!("0 0 {:.3} {:.3} re W n\n", w, h).as_bytes());
    content.extend_from_slice(b"BT\n");
    content.extend_from_slice(format!("/Helv {:.3} Tf\n", size).as_bytes());
    content.extend_from_slice(b"0 g\n");
    content.extend_from_slice(format!("2 {:.3} Td\n", baseline).as_bytes());
    content.push(b'(');
    content.extend_from_slice(&escaped);
    content.extend_from_slice(b") Tj\n");
    content.extend_from_slice(b"ET\nQ\nEMC\n");

    let mut font_dict = Dictionary::new();
    font_dict.set("Helv", Object::Reference(helv));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(font_dict));

    let mut xobj_dict = Dictionary::new();
    xobj_dict.set("Type", Object::Name(b"XObject".to_vec()));
    xobj_dict.set("Subtype", Object::Name(b"Form".to_vec()));
    xobj_dict.set("FormType", Object::Integer(1));
    xobj_dict.set(
        "BBox",
        Object::Array(vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(w as f32),
            Object::Real(h as f32),
        ]),
    );
    xobj_dict.set("Resources", Object::Dictionary(resources));

    let stream = Stream::new(xobj_dict, content);
    doc.add_object(Object::Stream(stream))
}

/// Set a checkbox's `/V` and `/AS` to the field's on-state name
/// (typically `/Yes`) or `/Off`. Returns `Ok(())` even if the field doesn't
/// exist — caller can use `index.has()` first if presence is required.
pub(super) fn set_checkbox(
    doc: &mut Document,
    index: &FieldIndex,
    name: &str,
    checked: bool,
) -> Result<(), PdfRenderError> {
    let id = index.get(name)?;
    let on_state = detect_on_state(doc, id).unwrap_or_else(|| b"Yes".to_vec());
    let state = if checked {
        Object::Name(on_state)
    } else {
        Object::Name(b"Off".to_vec())
    };
    let dict = doc
        .get_object_mut(id)
        .map_err(PdfRenderError::TemplateParse)?
        .as_dict_mut()
        .map_err(PdfRenderError::TemplateParse)?;
    dict.set("V", state.clone());
    // If this is a terminal field that is itself a widget, /AS sits on the
    // same dict. If it's a parent with widget kids, /AS must be set on each
    // kid.
    let has_kids = dict.has(b"Kids");
    if !has_kids {
        dict.set("AS", state);
    } else {
        // Collect kid IDs first to avoid borrow issues.
        let kid_ids: Vec<ObjectId> = dict
            .get(b"Kids")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        for kid_id in kid_ids {
            if let Ok(kid_obj) = doc.get_object_mut(kid_id) {
                if let Ok(kid_dict) = kid_obj.as_dict_mut() {
                    kid_dict.set("AS", state.clone());
                }
            }
        }
    }
    Ok(())
}

/// Inspect the field's appearance dictionary (`/AP /N`) to find the name of
/// the "on" state. Conventionally `/Yes`, but PDFs can use other names.
fn detect_on_state(doc: &Document, id: ObjectId) -> Option<Vec<u8>> {
    let dict = doc.get_dictionary(id).ok()?;
    // Try the field itself.
    if let Some(name) = on_state_from_dict(dict) {
        return Some(name);
    }
    // Otherwise inspect the first widget kid.
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            if let Ok(kid_id) = k.as_reference() {
                if let Ok(kid_dict) = doc.get_dictionary(kid_id) {
                    if let Some(name) = on_state_from_dict(kid_dict) {
                        return Some(name);
                    }
                }
            }
        }
    }
    None
}

fn on_state_from_dict(dict: &lopdf::Dictionary) -> Option<Vec<u8>> {
    let ap = dict.get(b"AP").ok()?.as_dict().ok()?;
    let n = ap.get(b"N").ok()?.as_dict().ok()?;
    for (k, _) in n.iter() {
        if k.as_slice() != b"Off" {
            return Some(k.to_vec());
        }
    }
    None
}

/// Final form-level tweaks before serialization: enable `/NeedAppearances`
/// for viewers that honor it, and fix the AcroForm root's `/DA` (the
/// template ships `(/Helv 0 Tf 0 g)`, where the size-zero font means
/// "auto-fit" — buggy viewers render fields blank instead).
pub(super) fn finalize_form(doc: &mut Document) {
    let Some(catalog_id) = catalog_id(doc) else {
        return;
    };
    let Ok(catalog) = doc.get_object_mut(catalog_id) else {
        return;
    };
    let Ok(catalog_dict) = catalog.as_dict_mut() else {
        return;
    };
    let form_id_or_inline = catalog_dict.get(b"AcroForm").cloned();
    match form_id_or_inline {
        Ok(Object::Reference(id)) => {
            if let Ok(form_obj) = doc.get_object_mut(id) {
                if let Ok(form_dict) = form_obj.as_dict_mut() {
                    apply_form_fixes(form_dict);
                }
            }
        }
        Ok(Object::Dictionary(mut d)) => {
            apply_form_fixes(&mut d);
            // Re-store the modified dict inline.
            if let Ok(catalog) = doc.get_object_mut(catalog_id) {
                if let Ok(catalog_dict) = catalog.as_dict_mut() {
                    catalog_dict.set("AcroForm", Object::Dictionary(d));
                }
            }
        }
        _ => {}
    }
}

fn apply_form_fixes(form_dict: &mut Dictionary) {
    form_dict.set("NeedAppearances", Object::Boolean(true));
    form_dict.set(
        "DA",
        Object::string_literal("/Helv 9 Tf 0 g".as_bytes().to_vec()),
    );
}

fn catalog_id(doc: &Document) -> Option<ObjectId> {
    let root = doc.trailer.get(b"Root").ok()?;
    root.as_reference().ok()
}
