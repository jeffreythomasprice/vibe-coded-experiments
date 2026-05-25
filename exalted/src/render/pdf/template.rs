//! Embedded PDF template bytes and loading.

use std::sync::OnceLock;

use lopdf::Document;

use super::PdfRenderError;

pub(super) const TEMPLATE_BYTES: &[u8] =
    include_bytes!("../../../assets/character-sheet/Exalted2ndED4-Page_TheSolarsV2_Editable.pdf");

/// Parse the embedded template once, then clone it on subsequent calls.
/// Parsing the 4.8 MB AcroForm dominates render time; cloning the in-memory
/// `Document` is dramatically cheaper than re-tokenizing the byte stream.
///
/// Uses `get_or_init` (rather than the get/set racy variant) so that
/// concurrent first calls — e.g. parallel `cargo test` threads — block on a
/// single parse instead of all racing through `load_mem` before anyone sets
/// the cache. Parse failure panics because `TEMPLATE_BYTES` is a compile-time
/// constant; if it fails to parse, every render would fail identically.
pub(super) fn load_template() -> Result<Document, PdfRenderError> {
    static CACHED: OnceLock<Document> = OnceLock::new();
    let doc = CACHED.get_or_init(|| {
        Document::load_mem(TEMPLATE_BYTES).expect("embedded PDF template should parse")
    });
    Ok(doc.clone())
}
