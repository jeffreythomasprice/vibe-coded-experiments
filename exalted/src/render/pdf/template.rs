//! Embedded PDF template bytes and loading.

use lopdf::Document;

use super::PdfRenderError;

pub(super) const TEMPLATE_BYTES: &[u8] =
    include_bytes!("../../../assets/character-sheet/Exalted2ndED4-Page_TheSolarsV2_Editable.pdf");

pub(super) fn load_template() -> Result<Document, PdfRenderError> {
    Document::load_mem(TEMPLATE_BYTES).map_err(PdfRenderError::TemplateParse)
}
