//! Drawing primitives that overlay raw PDF content onto a page, used when
//! AcroForm fields can't express what we want to render (e.g. the MrGone
//! sheet only carries six Essence dots — dots 7–10 are drawn here).

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::PdfRenderError;

/// Bézier coefficient for approximating a quarter-circle with one cubic
/// curve — see https://spencermortensen.com/articles/bezier-circle/.
const KAPPA: f64 = 0.5522847498307936;

/// Append PDF drawing operators for a circle of radius `r` centered at
/// `(cx, cy)`. Four cubic Béziers; filled with `f` or stroked with `S`.
pub(super) fn append_circle(out: &mut Vec<u8>, cx: f64, cy: f64, r: f64, filled: bool) {
    let k = KAPPA * r;
    let path = format!(
        "{:.3} {:.3} m\n\
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n\
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n\
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n\
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n",
        cx - r, cy,
        cx - r, cy + k, cx - k, cy + r, cx, cy + r,
        cx + k, cy + r, cx + r, cy + k, cx + r, cy,
        cx + r, cy - k, cx + k, cy - r, cx, cy - r,
        cx - k, cy - r, cx - r, cy - k, cx - r, cy,
    );
    out.extend_from_slice(path.as_bytes());
    out.extend_from_slice(if filled { b"f\n" } else { b"S\n" });
}

/// Find the page object that owns `widget_id`. Prefers the widget's own
/// `/P` reference; falls back to scanning each page's `/Annots` array for
/// templates that don't link widgets back to their parent page (the MrGone
/// sheet sets `/P` but leaves `/Annots` off the page dict entirely).
pub(super) fn page_containing_widget(doc: &Document, widget_id: ObjectId) -> Option<ObjectId> {
    if let Ok(dict) = doc.get_dictionary(widget_id) {
        if let Ok(p) = dict.get(b"P") {
            if let Ok(id) = p.as_reference() {
                return Some(id);
            }
        }
    }
    for (_, page_id) in doc.get_pages() {
        let Ok(page) = doc.get_dictionary(page_id) else {
            continue;
        };
        let Ok(annots) = page.get(b"Annots").and_then(|o| o.as_array()) else {
            continue;
        };
        for a in annots {
            if let Ok(aid) = a.as_reference() {
                if aid == widget_id {
                    return Some(page_id);
                }
            }
        }
    }
    None
}

/// Append `content_bytes` as a new content stream at the end of `page_id`'s
/// `/Contents`. The new stream is added after any existing streams, so it
/// draws on top of the existing page content (widgets are drawn separately
/// via their `/AP` and remain unaffected).
pub(super) fn append_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    content_bytes: Vec<u8>,
) -> Result<(), PdfRenderError> {
    let new_stream_id =
        doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content_bytes)));

    let page = doc
        .get_object_mut(page_id)
        .map_err(PdfRenderError::TemplateParse)?
        .as_dict_mut()
        .map_err(PdfRenderError::TemplateParse)?;

    let existing = page.get(b"Contents").ok().cloned();
    match existing {
        Some(Object::Reference(id)) => {
            page.set(
                "Contents",
                Object::Array(vec![Object::Reference(id), Object::Reference(new_stream_id)]),
            );
        }
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(new_stream_id));
            page.set("Contents", Object::Array(arr));
        }
        None => {
            page.set("Contents", Object::Reference(new_stream_id));
        }
        Some(_) => {
            return Err(PdfRenderError::Serialize(
                "page /Contents has unexpected type".to_string(),
            ));
        }
    }
    Ok(())
}
