//! Rendering of a `Character` into various output formats.
//!
//! - [`character_to_markdown`] — human-readable markdown sheet.
//! - [`character_to_pdf`] — filled PDF based on MrGone's 2e Solars template.

pub(crate) mod names;
mod markdown;
pub mod pdf;

pub use markdown::character_to_markdown;
pub use pdf::{character_to_pdf, PdfRenderError};
