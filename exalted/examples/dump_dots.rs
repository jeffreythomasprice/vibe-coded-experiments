//! One-shot helper for deriving the `dot_map` tables in
//! `src/render/pdf/dot_map.rs`.
//!
//! Walks the embedded MrGone editable Solars sheet's AcroForm tree, finds
//! every terminal field whose `/T` matches `^dot\d+$`, reads its widget
//! `/Rect`, computes the center coordinate, groups the dots by page and row
//! (with a ~3 PDF-unit tolerance), and prints a human-readable dump:
//!
//! ```text
//! page 1
//!   row 17  y=702.3  count=5   dot12 dot13 dot14 dot15 dot16
//!   row 18  y=688.5  count=5   dot17 dot18 dot19 dot20 dot21
//!   ...
//! ```
//!
//! The operator then uses the dump alongside the PDF to assign each row to
//! its category (Strength's 5 dots, Dexterity's 5 dots, …) and updates
//! `dot_map.rs` by hand.
//!
//! Run with: `cargo run --example dump_dots > /tmp/exalted-dots.txt`

use std::collections::HashMap;

use lopdf::{Document, Object, ObjectId};

const TEMPLATE_BYTES: &[u8] =
    include_bytes!("../assets/character-sheet/Exalted2ndED4-Page_TheSolarsV2_Editable.pdf");

/// Tolerance in PDF user-space units for clustering widgets into the same row.
const ROW_TOLERANCE: f64 = 3.0;

#[derive(Clone, Debug)]
struct Dot {
    name: String,
    page: u32,
    cx: f64,
    cy: f64,
}

fn main() {
    let doc = Document::load_mem(TEMPLATE_BYTES).expect("parse template");
    let page_of_widget = build_widget_page_index(&doc);

    let mut dots = Vec::<Dot>::new();
    walk_acroform(&doc, &page_of_widget, &mut dots);

    dots.sort_by(|a, b| {
        a.page
            .cmp(&b.page)
            .then(b.cy.partial_cmp(&a.cy).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.cx.partial_cmp(&b.cx).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut current_page: Option<u32> = None;
    let mut row_buffer: Vec<&Dot> = Vec::new();
    let mut row_y: f64 = 0.0;
    let mut row_idx: usize = 0;

    let flush_row = |out: &mut Vec<String>, row: &mut Vec<&Dot>, row_y: f64, row_idx: usize| {
        if row.is_empty() {
            return;
        }
        row.sort_by(|a, b| a.cx.partial_cmp(&b.cx).unwrap_or(std::cmp::Ordering::Equal));
        let names = row
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let xs = row
            .iter()
            .map(|d| format!("{:.0}", d.cx))
            .collect::<Vec<_>>()
            .join(",");
        out.push(format!(
            "  row {:>3}  y={:>6.1}  count={:>2}  x=[{}]  {}",
            row_idx,
            row_y,
            row.len(),
            xs,
            names
        ));
        row.clear();
    };

    let mut lines: Vec<String> = Vec::new();
    for dot in &dots {
        if current_page != Some(dot.page) {
            flush_row(&mut lines, &mut row_buffer, row_y, row_idx);
            current_page = Some(dot.page);
            row_idx = 0;
            lines.push(format!("page {}", dot.page + 1));
            row_y = dot.cy;
            row_buffer.push(dot);
            row_idx = 1;
            continue;
        }
        if !row_buffer.is_empty() && (row_y - dot.cy).abs() > ROW_TOLERANCE {
            flush_row(&mut lines, &mut row_buffer, row_y, row_idx);
            row_idx += 1;
            row_y = dot.cy;
        }
        row_buffer.push(dot);
        if row_buffer.len() == 1 {
            row_y = dot.cy;
        }
    }
    flush_row(&mut lines, &mut row_buffer, row_y, row_idx);

    println!("# dot mapping dump — {} dots total", dots.len());
    println!("# (run `cargo run --example dump_dots > /tmp/exalted-dots.txt`)");
    for line in &lines {
        println!("{}", line);
    }
}

/// Build a map from widget annotation `ObjectId` to 0-based page index by
/// walking each page's `/Annots`.
fn build_widget_page_index(doc: &Document) -> HashMap<ObjectId, u32> {
    let mut map = HashMap::new();
    let pages = doc.get_pages();
    for (page_num, page_id) in pages {
        let Ok(page_dict) = doc.get_dictionary(page_id) else {
            continue;
        };
        let Ok(annots_obj) = page_dict.get(b"Annots") else {
            continue;
        };
        let arr = match annots_obj {
            Object::Array(a) => a.clone(),
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(Object::Array(a)) => a.clone(),
                _ => continue,
            },
            _ => continue,
        };
        for entry in &arr {
            if let Ok(id) = entry.as_reference() {
                map.insert(id, page_num.saturating_sub(1));
            }
        }
    }
    map
}

fn walk_acroform(
    doc: &Document,
    widget_page: &HashMap<ObjectId, u32>,
    out: &mut Vec<Dot>,
) {
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
    let Ok(fields_arr) = form_dict.get(b"Fields").and_then(|o| o.as_array()) else {
        return;
    };
    for f in fields_arr {
        if let Ok(id) = f.as_reference() {
            walk_field(doc, widget_page, id, None, out);
        }
    }
}

fn walk_field(
    doc: &Document,
    widget_page: &HashMap<ObjectId, u32>,
    id: ObjectId,
    inherited: Option<&str>,
    out: &mut Vec<Dot>,
) {
    let Ok(dict) = doc.get_dictionary(id) else {
        return;
    };
    let local_name = dict.get(b"T").ok().and_then(|o| match o {
        Object::String(b, _) => std::str::from_utf8(b).ok().map(str::to_string),
        _ => None,
    });
    let full_name = match (inherited, local_name.as_deref()) {
        (None, Some(n)) => Some(n.to_string()),
        (Some(parent), Some(n)) => Some(format!("{}.{}", parent, n)),
        (Some(parent), None) => Some(parent.to_string()),
        (None, None) => None,
    };

    // Recurse if children are themselves fields (have /T).
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        let has_child_field = kids.iter().any(|k| {
            k.as_reference()
                .ok()
                .and_then(|kid_id| doc.get_dictionary(kid_id).ok())
                .map(|d| d.has(b"T"))
                .unwrap_or(false)
        });
        if has_child_field {
            for k in kids {
                if let Ok(kid_id) = k.as_reference() {
                    walk_field(doc, widget_page, kid_id, full_name.as_deref(), out);
                }
            }
            return;
        }
    }

    // Leaf field. Match dotN names.
    let Some(name) = full_name else { return };
    if !is_dot_name(&name) {
        return;
    }

    let (cx, cy, page) = read_widget_rect(doc, widget_page, id, dict).unwrap_or((0.0, 0.0, 0));
    out.push(Dot { name, page, cx, cy });
}

fn is_dot_name(name: &str) -> bool {
    // Match `dotN`, `willdotN`, `essencedotN`, `e2dotN`, `IdotN`, `xtdotN`,
    // `virtuecheckN`, `LBCheckN`, `healthcheckN`, `skillscheckN`, `willcheckN`,
    // `EPCheckN`, `APCheckN`, `FcheckN`, `LCheckN` — anything that could be
    // a rating/track checkbox.
    let prefixes = [
        "dot", "willdot", "essencedot", "e2dot", "Idot", "xtdot",
        "virtuecheck", "LBCheck", "healthcheck", "skillscheck", "willcheck",
        "EPCheck", "APCheck", "Fcheck", "LCheck",
    ];
    for p in prefixes {
        if let Some(rest) = name.strip_prefix(p) {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

fn read_widget_rect(
    doc: &Document,
    widget_page: &HashMap<ObjectId, u32>,
    id: ObjectId,
    dict: &lopdf::Dictionary,
) -> Option<(f64, f64, u32)> {
    // Terminal field may itself be a widget annotation (has /Rect) or have
    // widget kids. Try the field's own /Rect first.
    if let Some(r) = rect_center(dict) {
        let page = widget_page.get(&id).copied().unwrap_or(0);
        return Some((r.0, r.1, page));
    }
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            if let Ok(kid_id) = k.as_reference() {
                if let Ok(kid_dict) = doc.get_dictionary(kid_id) {
                    if let Some(r) = rect_center(kid_dict) {
                        let page = widget_page.get(&kid_id).copied().unwrap_or(0);
                        return Some((r.0, r.1, page));
                    }
                }
            }
        }
    }
    None
}

fn rect_center(dict: &lopdf::Dictionary) -> Option<(f64, f64)> {
    let rect_obj = dict.get(b"Rect").ok()?;
    let arr = rect_obj.as_array().ok()?;
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
    let llx = n(&arr[0])?;
    let lly = n(&arr[1])?;
    let urx = n(&arr[2])?;
    let ury = n(&arr[3])?;
    Some(((llx + urx) / 2.0, (lly + ury) / 2.0))
}
