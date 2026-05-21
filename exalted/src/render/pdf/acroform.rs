//! Low-level AcroForm field plumbing on top of `lopdf`.

use std::collections::HashMap;

use lopdf::{Document, Object, ObjectId};

use super::PdfRenderError;

/// Index of AcroForm field name → terminal field object ID.
///
/// AcroForm fields form a tree (`/Fields` with `/Kids`), but the leaves we
/// want to write — text fields, checkboxes — are identified by their fully
/// qualified `/T` name. Field names in this template are not nested
/// (no parent/child dotted paths) so a simple flat name table suffices.
pub(super) struct FieldIndex {
    by_name: HashMap<String, ObjectId>,
}

impl FieldIndex {
    pub(super) fn build(doc: &Document) -> Self {
        let mut by_name = HashMap::new();
        if let Some(root_id) = acroform_fields_root(doc) {
            walk(doc, root_id, &mut by_name);
        }
        FieldIndex { by_name }
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
    let Ok(form_obj) = catalog.get(b"AcroForm") else { return };
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
    let local_name = dict
        .get(b"T")
        .ok()
        .and_then(|o| match o {
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

/// Set the value (`/V`) of a text field, and clear any cached appearance
/// stream so the viewer regenerates it.
pub(super) fn set_text_field(
    doc: &mut Document,
    index: &FieldIndex,
    name: &str,
    value: &str,
) -> Result<(), PdfRenderError> {
    let id = index.get(name)?;
    let dict = doc
        .get_object_mut(id)
        .map_err(PdfRenderError::TemplateParse)?
        .as_dict_mut()
        .map_err(PdfRenderError::TemplateParse)?;
    dict.set("V", Object::string_literal(value));
    // Clear cached appearance — viewers honoring NeedAppearances will
    // regenerate from /V.
    dict.remove(b"AP");
    Ok(())
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

/// Sets `/AcroForm /NeedAppearances true` on the catalog so that PDF viewers
/// regenerate field appearance streams when opening the document.
pub(super) fn enable_need_appearances(doc: &mut Document) {
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
                    form_dict.set("NeedAppearances", Object::Boolean(true));
                }
            }
        }
        Ok(Object::Dictionary(mut d)) => {
            d.set("NeedAppearances", Object::Boolean(true));
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

fn catalog_id(doc: &Document) -> Option<ObjectId> {
    let root = doc.trailer.get(b"Root").ok()?;
    root.as_reference().ok()
}
