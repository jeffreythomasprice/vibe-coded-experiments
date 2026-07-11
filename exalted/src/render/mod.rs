//! Rendering of a `Character` into various output formats.
//!
//! - [`character_to_markdown`] — human-readable markdown sheet.
//! - [`character_to_pdf`] — filled PDF based on MrGone's 2e Solars template.

mod markdown;
pub(crate) mod names;
pub mod pdf;
pub mod rules_data;

pub use markdown::character_to_markdown;
pub use pdf::{PdfRenderError, character_to_pdf};
pub use rules_data::{
    art_to_markdown, arts_to_markdown, background_to_markdown, backgrounds_to_markdown,
    charm_to_markdown, charms_to_markdown, spell_to_markdown, spells_to_markdown,
};
