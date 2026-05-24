//! PDF rendering of a `Character` into the MrGone 2e Solars sheet.
//!
//! Loads the embedded editable PDF template from
//! `assets/character-sheet/Exalted2ndED4-Page_TheSolarsV2_Editable.pdf`,
//! fills its AcroForm fields, and serializes the result.

mod acroform;
mod checkboxes;
mod dots;
mod field_map;
mod notes_page;
mod overlay;
mod specialties;
mod template;
mod text_fields;

use crate::character::Character;

#[derive(Debug, thiserror::Error)]
pub enum PdfRenderError {
    #[error("failed to parse embedded PDF template: {0}")]
    TemplateParse(#[from] lopdf::Error),
    #[error("AcroForm field {0:?} not found in template")]
    MissingField(String),
    #[error("failed to serialize filled PDF: {0}")]
    Serialize(String),
}

/// Fill the embedded template with `c`'s data and return the resulting PDF
/// bytes. The caller is responsible for writing them to disk.
pub fn character_to_pdf(c: &Character) -> Result<Vec<u8>, PdfRenderError> {
    let mut doc = template::load_template()?;
    let index = acroform::FieldIndex::build(&doc);

    text_fields::fill(&mut doc, &index, c)?;
    checkboxes::fill(&mut doc, &index, c)?;
    dots::fill(&mut doc, &index, c)?;
    notes_page::append(&mut doc, c)?;
    acroform::finalize_form(&mut doc);

    let mut buf = Vec::new();
    doc.save_to(&mut buf)
        .map_err(|e| PdfRenderError::Serialize(e.to_string()))?;
    Ok(buf)
}
