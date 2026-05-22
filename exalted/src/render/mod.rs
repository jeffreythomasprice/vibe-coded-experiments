//! Rendering of a `Character` into various output formats.
//!
//! - [`character_to_markdown`] — human-readable markdown sheet.
//! - [`character_to_pdf`] — filled PDF based on MrGone's 2e Solars template.

pub(crate) mod names;
mod markdown;
pub mod pdf;
pub mod rules_data;

pub use markdown::character_to_markdown;
pub use pdf::{character_to_pdf, PdfRenderError};
pub use rules_data::{
    background_to_markdown, backgrounds_to_markdown, charm_to_markdown, charms_to_markdown,
    spell_to_markdown, spells_to_markdown,
};
