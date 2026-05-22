//! Markdown rendering for raw rules-database entries — used by the
//! `backgrounds` / `charms` / `spells` CLI subcommands when the caller asks
//! for human-readable output. JSON output is handled directly in `main.rs`
//! by serializing the entries via `serde_json` (the structs already derive
//! `Serialize`); these helpers only cover the markdown side.
//!
//! These are intentionally separate from `character_to_markdown` in
//! `render::markdown`, which formats *a character's* picks for the printed
//! sheet (compact bullet lists, tables). Here we want the full reference
//! text for each entry: every classifier field plus the description.

use std::fmt::Write;

use crate::character::BackgroundKind;
use crate::render::names::{attr_name, spell_circle_label};
use crate::rules::database::{BackgroundEntry, CharmEntry, SpellEntry};

// --------------------------------------------------------------------------
// Single-entry renderers
// --------------------------------------------------------------------------

pub fn background_to_markdown(b: &BackgroundEntry) -> String {
    let mut out = String::new();
    writeln!(out, "## {} (`{}`)", b.name, b.id).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "- **Kind:** {}", background_kind_label(b.kind)).unwrap();
    writeln!(out, "- **Source:** {} p.{}", b.source, b.pages).unwrap();
    writeln!(out).unwrap();
    write_description(&b.description, &mut out);
    out
}

pub fn charm_to_markdown(c: &CharmEntry) -> String {
    let mut out = String::new();
    writeln!(out, "## {} (`{}`)", c.name, c.id).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "- **Exalt:** {}", c.exalt_type).unwrap();
    writeln!(out, "- **Ability:** {}", c.ability).unwrap();
    writeln!(out, "- **Cost:** {}", c.cost).unwrap();
    writeln!(
        out,
        "- **Mins:** {} {}, Essence {}",
        c.ability, c.mins_ability, c.mins_essence
    )
    .unwrap();
    if !c.mins_attribute.is_empty() {
        let parts: Vec<String> = c
            .mins_attribute
            .iter()
            .map(|(k, v)| format!("{} {}", attr_name(*k), v))
            .collect();
        writeln!(out, "- **Min Attributes:** {}", parts.join(", ")).unwrap();
    }
    let type_line = if c.type_detail.is_empty() {
        c.charm_type.display().to_string()
    } else {
        format!("{} ({})", c.charm_type.display(), c.type_detail)
    };
    writeln!(out, "- **Type:** {}", type_line).unwrap();
    writeln!(
        out,
        "- **Keywords:** {}",
        if c.keywords.is_empty() {
            "—".to_string()
        } else {
            c.keywords.join(", ")
        }
    )
    .unwrap();
    writeln!(out, "- **Duration:** {}", c.duration).unwrap();
    writeln!(
        out,
        "- **Prerequisites:** {}",
        if c.prerequisites.is_empty() {
            "—".to_string()
        } else {
            c.prerequisites.join(", ")
        }
    )
    .unwrap();
    writeln!(out, "- **Source:** {} p.{}", c.source, c.pages).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "**Effect:** {}", c.effect).unwrap();
    writeln!(out).unwrap();
    write_description(&c.description, &mut out);
    out
}

pub fn spell_to_markdown(s: &SpellEntry) -> String {
    let mut out = String::new();
    writeln!(out, "## {} (`{}`)", s.name, s.id).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "- **Circle:** {}", spell_circle_label(s.circle)).unwrap();
    writeln!(out, "- **Cost:** {}", s.cost).unwrap();
    writeln!(
        out,
        "- **Keywords:** {}",
        if s.keywords.is_empty() {
            "—".to_string()
        } else {
            s.keywords.join(", ")
        }
    )
    .unwrap();
    writeln!(out, "- **Duration:** {}", s.duration).unwrap();
    writeln!(out, "- **Target:** {}", s.target).unwrap();
    writeln!(out, "- **Source:** {} p.{}", s.source, s.pages).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "**Effect:** {}", s.effect).unwrap();
    writeln!(out).unwrap();
    write_description(&s.description, &mut out);
    out
}

// --------------------------------------------------------------------------
// List renderers
// --------------------------------------------------------------------------

pub fn backgrounds_to_markdown(entries: &[&BackgroundEntry]) -> String {
    join_sections(entries.iter().map(|b| background_to_markdown(b)))
}

pub fn charms_to_markdown(entries: &[&CharmEntry]) -> String {
    join_sections(entries.iter().map(|c| charm_to_markdown(c)))
}

pub fn spells_to_markdown(entries: &[&SpellEntry]) -> String {
    join_sections(entries.iter().map(|s| spell_to_markdown(s)))
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn join_sections<I: Iterator<Item = String>>(sections: I) -> String {
    let mut out = String::new();
    for section in sections {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&section);
    }
    out
}

/// Append a description block, normalizing whitespace so the rendered
/// document doesn't acquire stray leading/trailing blank lines from the
/// TOML triple-quoted heredocs. Always terminates with a single blank line.
fn write_description(desc: &str, out: &mut String) {
    let trimmed = desc.trim();
    if !trimmed.is_empty() {
        out.push_str(trimmed);
        out.push('\n');
        out.push('\n');
    }
}

fn background_kind_label(kind: BackgroundKind) -> &'static str {
    match kind {
        BackgroundKind::Allies => "Allies",
        BackgroundKind::Artifact => "Artifact",
        BackgroundKind::Backing => "Backing",
        BackgroundKind::Contacts => "Contacts",
        BackgroundKind::Cult => "Cult",
        BackgroundKind::Familiar => "Familiar",
        BackgroundKind::Followers => "Followers",
        BackgroundKind::Influence => "Influence",
        BackgroundKind::Manse => "Manse",
        BackgroundKind::Mentor => "Mentor",
        BackgroundKind::Resources => "Resources",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::database::database;

    #[test]
    fn background_renders_kind_and_id() {
        let db = database();
        let bg = db.background("allies").expect("allies background exists");
        let md = background_to_markdown(bg);
        assert!(md.contains("## Allies (`allies`)"));
        assert!(md.contains("**Kind:** Allies"));
        assert!(md.contains("**Source:**"));
        assert!(md.contains("close friends"), "description missing: {md}");
    }

    #[test]
    fn charm_renders_all_field_labels() {
        let db = database();
        // An excellency-derived id confirms the expansion is visible to
        // this renderer too.
        let c = db
            .charm("first-archery-excellency")
            .expect("first-archery-excellency exists after expansion");
        let md = charm_to_markdown(c);
        assert!(md.contains("## First Archery Excellency"));
        assert!(md.contains("(`first-archery-excellency`)"));
        assert!(md.contains("**Ability:** Archery"));
        assert!(md.contains("**Cost:**"));
        assert!(md.contains("**Type:** Reflexive"));
        assert!(md.contains("**Mins:** Archery 1, Essence 1"));
        assert!(md.contains("**Keywords:** Combo-OK"));
        assert!(md.contains("**Effect:**"));
    }

    #[test]
    fn spell_renders_circle_label() {
        let db = database();
        let s = db
            .spell("assassins-fatal-touch")
            .expect("assassins-fatal-touch exists");
        let md = spell_to_markdown(s);
        assert!(md.contains("## Assassin's Fatal Touch"));
        assert!(md.contains("**Circle:** Terrestrial"));
        assert!(md.contains("**Cost:** 20m"));
        assert!(md.contains("**Keywords:** Poison, Touch"));
        assert!(md.contains("**Target:** Touched creature"));
    }

    #[test]
    fn empty_keyword_or_prereq_lists_render_dash() {
        let db = database();
        // Find any charm with no keywords or no prereqs.
        let no_kw = db.iter_charms().find(|c| c.keywords.is_empty());
        if let Some(c) = no_kw {
            assert!(charm_to_markdown(c).contains("**Keywords:** —"));
        }
        let no_pre = db.iter_charms().find(|c| c.prerequisites.is_empty());
        if let Some(c) = no_pre {
            assert!(charm_to_markdown(c).contains("**Prerequisites:** —"));
        }
    }

    #[test]
    fn list_renderer_sorts_by_caller_and_separates_sections() {
        // The list renderers don't sort themselves; main.rs does. Verify
        // that we faithfully emit in the given order with blank-line
        // separators.
        let db = database();
        let a = db.background("allies").unwrap();
        let b = db.background("artifact").unwrap();
        let md = backgrounds_to_markdown(&[a, b]);
        let pos_a = md.find("## Allies").expect("Allies present");
        let pos_b = md.find("## Artifact").expect("Artifact present");
        assert!(pos_a < pos_b);
        // Blank line between sections.
        assert!(md.contains("\n\n## Artifact"));
    }
}
