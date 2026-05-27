//! UI search/find. Builds an index of "searchable items" from the character
//! and a static catalogue of section labels, supports navigation with
//! prev/next, force-opens collapsed parents on focus, and offers helpers for
//! sections / widgets to apply the highlight visuals.

use std::sync::Arc;

use crate::character::{
    AbilityKind, AttributeGroup, AttributeKind, BackgroundRef, Caste, Character, CharmRef,
    KnownLanguage, LanguageFamily, SpellRef, VirtueFlaw, VirtueKind,
};
use crate::render::names::{ability_name, attr_name, caste_name, group_name, virtue_name};
use crate::rules::database::database;

// -----------------------------------------------------------------------------
// Match-target identification
// -----------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    Match,
    Focused,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SectionId {
    Identity,
    Attributes,
    Abilities,
    Virtues,
    WillpowerEssence,
    Charms,
    Combos,
    Spells,
    Backgrounds,
    Intimacies,
    Familiar,
    Equipment,
    Hearthstones,
    Languages,
    Pools,
    Xp,
    Notes,
}

impl SectionId {
    pub fn label(self) -> &'static str {
        match self {
            SectionId::Identity => "Identity",
            SectionId::Attributes => "Attributes",
            SectionId::Abilities => "Abilities",
            SectionId::Virtues => "Virtues",
            SectionId::WillpowerEssence => "Willpower & Essence",
            SectionId::Charms => "Charms",
            SectionId::Combos => "Combos",
            SectionId::Spells => "Spells",
            SectionId::Backgrounds => "Backgrounds",
            SectionId::Intimacies => "Intimacies",
            SectionId::Familiar => "Familiar",
            SectionId::Equipment => "Equipment",
            SectionId::Hearthstones => "Hearthstones",
            SectionId::Languages => "Languages",
            SectionId::Pools => "Pools & In-Play State",
            SectionId::Xp => "Experience",
            SectionId::Notes => "Notes",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IdentityField {
    Name,
    Player,
    Chronicle,
    Concept,
    Motivation,
    Personality,
    AnimaTotem,
    AppearanceSex,
    AppearanceHair,
    AppearanceEyes,
    AppearanceSkin,
    AppearanceHomeland,
    AppearanceFeatures,
    VirtueFlawName,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum NamedField {
    Name,
    Description,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BgField {
    Label,
    CustomName,
    CustomDescription,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArtifactField {
    Name,
    Description,
    Sockets,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum HsField {
    Name,
    Aspect,
    Description,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum NoteParent {
    Character,
    Charm(usize),
    Spell(usize),
    Background(usize),
    Combo(usize),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PossessionField {
    Name,
    Primary,
    Secondary,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WeaponField {
    Name,
    Tags,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EquipmentSubsection {
    Weapons,
    Armor,
    OtherPossessions,
    Artifacts,
}

impl EquipmentSubsection {
    pub const ALL: &'static [EquipmentSubsection] = &[
        EquipmentSubsection::Weapons,
        EquipmentSubsection::Armor,
        EquipmentSubsection::OtherPossessions,
        EquipmentSubsection::Artifacts,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EquipmentSubsection::Weapons => "Weapons",
            EquipmentSubsection::Armor => "Armor",
            EquipmentSubsection::OtherPossessions => "Other possessions",
            EquipmentSubsection::Artifacts => "Artifacts",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MatchTarget {
    SectionHeading(SectionId),
    AttributeLabel(AttributeKind),
    AttributeGroupHeading(AttributeGroup),
    AbilityLabel(AbilityKind),
    CasteHeading(Caste),
    VirtueLabel(VirtueKind),
    Identity(IdentityField),
    Specialty { ability: AbilityKind, idx: usize },
    Background { idx: usize, field: BgField },
    Note { parent: NoteParent, idx: usize },
    Charm { idx: usize, field: NamedField },
    Spell { idx: usize, field: NamedField },
    Combo { idx: usize },
    Intimacy(usize),
    EquipmentSubsectionHeading(EquipmentSubsection),
    Weapon { idx: usize, field: WeaponField },
    Armor,
    Possession { idx: usize, field: PossessionField },
    Artifact { idx: usize, field: ArtifactField },
    Hearthstone { idx: usize, field: HsField },
    LanguageTribal(usize),
    LanguageDialect(usize),
    FamiliarName,
    XpAward(usize),
}

// -----------------------------------------------------------------------------
// SearchState
// -----------------------------------------------------------------------------

#[derive(Default)]
pub struct SearchState {
    pub visible: bool,
    pub query: String,
    pub results: Vec<MatchTarget>,
    pub current_index: usize,
    pub focus_input: bool,
    pub scroll_pending: bool,
    last_query: String,
}

impl SearchState {
    /// Open the search bar, or if already open, re-focus the input and select
    /// all so the user can just type to replace the current query. Closing is
    /// done via Esc or the ✕ button, never via this entry point.
    pub fn open(&mut self) {
        self.visible = true;
        self.focus_input = true;
        self.scroll_pending = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.results.clear();
        self.current_index = 0;
        self.scroll_pending = false;
    }

    pub fn next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.current_index = (self.current_index + 1) % self.results.len();
        self.scroll_pending = true;
    }

    pub fn prev(&mut self) {
        if self.results.is_empty() {
            return;
        }
        if self.current_index == 0 {
            self.current_index = self.results.len() - 1;
        } else {
            self.current_index -= 1;
        }
        self.scroll_pending = true;
    }

    pub fn focused(&self) -> Option<MatchTarget> {
        self.results.get(self.current_index).copied()
    }

    pub fn highlight_for(&self, target: MatchTarget) -> Option<HighlightKind> {
        if !self.visible || self.results.is_empty() {
            return None;
        }
        if self.results.get(self.current_index) == Some(&target) {
            return Some(HighlightKind::Focused);
        }
        if self.results.iter().any(|t| t == &target) {
            return Some(HighlightKind::Match);
        }
        None
    }

    /// Is the currently focused match anywhere in the set described by
    /// `predicate`? Used by container widgets to force-open themselves when a
    /// child match is focused.
    pub fn focused_within(&self, predicate: impl Fn(&MatchTarget) -> bool) -> bool {
        self.focused().map(|t| predicate(&t)).unwrap_or(false)
    }

    /// Recompute the result list against the live character. Cheap; called
    /// once per frame while the search bar is visible.
    pub fn recompute(&mut self, character: &Character) {
        let prev_focused = self.focused();
        let query_changed = self.query != self.last_query;
        self.last_query = self.query.clone();

        self.results.clear();
        let q = self.query.trim();
        if q.is_empty() {
            self.current_index = 0;
            return;
        }
        let qlc = q.to_lowercase();
        collect_matches(character, &qlc, &mut self.results);

        // Try to keep focus on whatever the user was looking at; otherwise
        // clamp to the new (possibly shorter) list.
        if let Some(prev) = prev_focused {
            if let Some(i) = self.results.iter().position(|t| t == &prev) {
                self.current_index = i;
            } else {
                self.current_index = self.current_index.min(self.results.len().saturating_sub(1));
                if query_changed {
                    self.scroll_pending = true;
                }
            }
        } else {
            self.current_index = self.current_index.min(self.results.len().saturating_sub(1));
            if query_changed && !self.results.is_empty() {
                self.scroll_pending = true;
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Match collection
// -----------------------------------------------------------------------------

fn matches(text: &str, qlc: &str) -> bool {
    !text.is_empty() && text.to_lowercase().contains(qlc)
}

fn collect_matches(character: &Character, qlc: &str, out: &mut Vec<MatchTarget>) {
    use MatchTarget as MT;

    // ----- Identity -----
    if matches(SectionId::Identity.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Identity));
    }
    let id_pairs: &[(IdentityField, &str)] = &[
        (IdentityField::Name, &character.identity.name),
        (IdentityField::Player, &character.identity.player),
        (IdentityField::Chronicle, &character.identity.chronicle),
        (IdentityField::Concept, &character.identity.concept),
        (IdentityField::Motivation, &character.identity.motivation),
        (IdentityField::Personality, &character.identity.personality),
        (IdentityField::AnimaTotem, &character.identity.anima.totem),
        (
            IdentityField::AppearanceSex,
            &character.identity.appearance.sex,
        ),
        (
            IdentityField::AppearanceHair,
            &character.identity.appearance.hair,
        ),
        (
            IdentityField::AppearanceEyes,
            &character.identity.appearance.eyes,
        ),
        (
            IdentityField::AppearanceSkin,
            &character.identity.appearance.skin,
        ),
        (
            IdentityField::AppearanceHomeland,
            &character.identity.appearance.homeland,
        ),
        (
            IdentityField::AppearanceFeatures,
            &character.identity.appearance.distinguishing_features,
        ),
    ];
    for (f, t) in id_pairs {
        if matches(t, qlc) {
            out.push(MT::Identity(*f));
        }
    }
    if let Some(VirtueFlaw::Custom { name, .. }) = &character.virtue_flaw {
        if matches(name, qlc) {
            out.push(MT::Identity(IdentityField::VirtueFlawName));
        }
    }

    // ----- Attributes -----
    if matches(SectionId::Attributes.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Attributes));
    }
    // Attribute group headings (Physical / Social / Mental): when the query
    // matches a group name, surface both the heading and all of its
    // attributes so they light up together.
    let mut group_attr_matches: std::collections::HashSet<AttributeKind> =
        std::collections::HashSet::new();
    for group in AttributeGroup::ALL {
        if matches(group_name(*group), qlc) {
            out.push(MT::AttributeGroupHeading(*group));
            for attr in group.members() {
                group_attr_matches.insert(*attr);
            }
        }
    }
    for kind in AttributeKind::ALL {
        if matches(attr_name(*kind), qlc) || group_attr_matches.contains(kind) {
            out.push(MT::AttributeLabel(*kind));
        }
    }

    // ----- Abilities -----
    if matches(SectionId::Abilities.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Abilities));
    }
    // Caste headings: when the query matches a caste name (Dawn, Zenith, …),
    // surface both the heading and all five of its caste abilities so they
    // light up together.
    let mut caste_ability_matches: std::collections::HashSet<AbilityKind> =
        std::collections::HashSet::new();
    for caste in [
        Caste::Dawn,
        Caste::Zenith,
        Caste::Twilight,
        Caste::Night,
        Caste::Eclipse,
    ] {
        if matches(caste_name(caste), qlc) {
            out.push(MT::CasteHeading(caste));
            for ab in caste.caste_abilities() {
                caste_ability_matches.insert(*ab);
            }
        }
    }
    for kind in AbilityKind::ALL {
        if matches(ability_name(*kind), qlc) || caste_ability_matches.contains(kind) {
            out.push(MT::AbilityLabel(*kind));
        }
        if let Some(trait_) = character.abilities.get(kind) {
            for (i, sp) in trait_.specialties.iter().enumerate() {
                if matches(&sp.name, qlc) {
                    out.push(MT::Specialty {
                        ability: *kind,
                        idx: i,
                    });
                }
            }
        }
    }

    // ----- Virtues -----
    if matches(SectionId::Virtues.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Virtues));
    }
    for v in VirtueKind::ALL {
        if matches(virtue_name(*v), qlc) {
            out.push(MT::VirtueLabel(*v));
        }
    }

    // ----- Willpower & Essence -----
    if matches(SectionId::WillpowerEssence.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::WillpowerEssence));
    }

    // ----- Charms -----
    if matches(SectionId::Charms.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Charms));
    }
    let db = database();
    for (i, charm) in character.charms.iter().enumerate() {
        if matches(charm.display_name(db), qlc) {
            out.push(MT::Charm {
                idx: i,
                field: NamedField::Name,
            });
        }
        if let CharmRef::Custom { entry, .. } = charm {
            if matches(&entry.description, qlc) {
                out.push(MT::Charm {
                    idx: i,
                    field: NamedField::Description,
                });
            }
        }
        let notes = match charm {
            CharmRef::Lookup { notes, .. } => notes,
            CharmRef::Custom { notes, .. } => notes,
        };
        for (ni, note) in notes.iter().enumerate() {
            if matches(&note.body, qlc) {
                out.push(MT::Note {
                    parent: NoteParent::Charm(i),
                    idx: ni,
                });
            }
        }
    }

    // ----- Combos -----
    if matches(SectionId::Combos.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Combos));
    }
    for (i, combo) in character.combos.iter().enumerate() {
        if matches(&combo.name, qlc) {
            out.push(MT::Combo { idx: i });
        }
        for (ni, note) in combo.notes.iter().enumerate() {
            if matches(&note.body, qlc) {
                out.push(MT::Note {
                    parent: NoteParent::Combo(i),
                    idx: ni,
                });
            }
        }
    }

    // ----- Spells -----
    if matches(SectionId::Spells.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Spells));
    }
    for (i, spell) in character.spells.iter().enumerate() {
        if matches(spell.display_name(db), qlc) {
            out.push(MT::Spell {
                idx: i,
                field: NamedField::Name,
            });
        }
        if let SpellRef::Custom { entry, .. } = spell {
            if matches(&entry.description, qlc) {
                out.push(MT::Spell {
                    idx: i,
                    field: NamedField::Description,
                });
            }
        }
        let notes = match spell {
            SpellRef::Lookup { notes, .. } => notes,
            SpellRef::Custom { notes, .. } => notes,
        };
        for (ni, note) in notes.iter().enumerate() {
            if matches(&note.body, qlc) {
                out.push(MT::Note {
                    parent: NoteParent::Spell(i),
                    idx: ni,
                });
            }
        }
    }

    // ----- Backgrounds -----
    if matches(SectionId::Backgrounds.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Backgrounds));
    }
    for (i, bg) in character.backgrounds.iter().enumerate() {
        // Display name covers both lookup (database) and custom variants.
        if matches(bg.display_name(db), qlc) {
            out.push(MT::Background {
                idx: i,
                field: BgField::CustomName,
            });
        }
        if matches(bg.label(), qlc) {
            out.push(MT::Background {
                idx: i,
                field: BgField::Label,
            });
        }
        if let BackgroundRef::Custom { entry, .. } = bg {
            if matches(&entry.description, qlc) {
                out.push(MT::Background {
                    idx: i,
                    field: BgField::CustomDescription,
                });
            }
        }
        let notes = match bg {
            BackgroundRef::Lookup { notes, .. } => notes,
            BackgroundRef::Custom { notes, .. } => notes,
        };
        for (ni, note) in notes.iter().enumerate() {
            if matches(&note.body, qlc) {
                out.push(MT::Note {
                    parent: NoteParent::Background(i),
                    idx: ni,
                });
            }
        }
    }

    // ----- Intimacies -----
    if matches(SectionId::Intimacies.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Intimacies));
    }
    for (i, intimacy) in character.intimacies.iter().enumerate() {
        if matches(&intimacy.description, qlc) {
            out.push(MT::Intimacy(i));
        }
    }

    // ----- Familiar -----
    if matches(SectionId::Familiar.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Familiar));
    }
    if let Some(f) = &character.familiar {
        if matches(&f.name, qlc) {
            out.push(MT::FamiliarName);
        }
    }

    // ----- Equipment -----
    if matches(SectionId::Equipment.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Equipment));
    }
    for sub in EquipmentSubsection::ALL {
        if matches(sub.label(), qlc) {
            out.push(MT::EquipmentSubsectionHeading(*sub));
        }
    }
    for (i, w) in character.equipment.weapons.iter().enumerate() {
        if matches(&w.name, qlc) {
            out.push(MT::Weapon {
                idx: i,
                field: WeaponField::Name,
            });
        }
        let tags_joined = w.tags.join(", ");
        if matches(&tags_joined, qlc) {
            out.push(MT::Weapon {
                idx: i,
                field: WeaponField::Tags,
            });
        }
    }
    if let Some(armor) = &character.equipment.armor {
        if matches(&armor.name, qlc) {
            out.push(MT::Armor);
        }
    }
    for (i, p) in character.equipment.other.iter().enumerate() {
        if matches(&p.name, qlc) {
            out.push(MT::Possession {
                idx: i,
                field: PossessionField::Name,
            });
        }
        if matches(&p.primary_location, qlc) {
            out.push(MT::Possession {
                idx: i,
                field: PossessionField::Primary,
            });
        }
        if let Some(s) = &p.secondary_location {
            if matches(s, qlc) {
                out.push(MT::Possession {
                    idx: i,
                    field: PossessionField::Secondary,
                });
            }
        }
    }
    for (i, a) in character.equipment.artifacts.iter().enumerate() {
        if matches(&a.name, qlc) {
            out.push(MT::Artifact {
                idx: i,
                field: ArtifactField::Name,
            });
        }
        let sockets = a.socketed_hearthstones.join(", ");
        if matches(&sockets, qlc) {
            out.push(MT::Artifact {
                idx: i,
                field: ArtifactField::Sockets,
            });
        }
        if matches(&a.description, qlc) {
            out.push(MT::Artifact {
                idx: i,
                field: ArtifactField::Description,
            });
        }
    }

    // ----- Hearthstones -----
    if matches(SectionId::Hearthstones.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Hearthstones));
    }
    for (i, hs) in character.hearthstones.iter().enumerate() {
        if matches(&hs.name, qlc) {
            out.push(MT::Hearthstone {
                idx: i,
                field: HsField::Name,
            });
        }
        if matches(&hs.aspect, qlc) {
            out.push(MT::Hearthstone {
                idx: i,
                field: HsField::Aspect,
            });
        }
        if matches(&hs.description, qlc) {
            out.push(MT::Hearthstone {
                idx: i,
                field: HsField::Description,
            });
        }
    }

    // ----- Languages -----
    if matches(SectionId::Languages.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Languages));
    }
    for (i, lang) in character.languages.iter().enumerate() {
        if let KnownLanguage {
            family: LanguageFamily::TribalTongue(name),
            ..
        } = lang
        {
            if matches(name, qlc) {
                out.push(MT::LanguageTribal(i));
            }
        }
        if let Some(d) = &lang.dialect_specialty {
            if matches(d, qlc) {
                out.push(MT::LanguageDialect(i));
            }
        }
    }

    // ----- Pools -----
    if matches(SectionId::Pools.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Pools));
    }

    // ----- XP -----
    if matches(SectionId::Xp.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Xp));
    }
    for (i, award) in character.xp_awards.iter().enumerate() {
        if matches(&award.body, qlc) {
            out.push(MT::XpAward(i));
        }
    }

    // ----- Notes (character-level) -----
    if matches(SectionId::Notes.label(), qlc) {
        out.push(MT::SectionHeading(SectionId::Notes));
    }
    for (i, note) in character.notes.iter().enumerate() {
        if matches(&note.body, qlc) {
            out.push(MT::Note {
                parent: NoteParent::Character,
                idx: i,
            });
        }
    }
}

// -----------------------------------------------------------------------------
// Highlight rendering helpers
// -----------------------------------------------------------------------------

fn match_bg() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(255, 220, 60, 180)
}

fn focused_bg() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(255, 140, 0, 200)
}

pub fn highlight_fill(kind: HighlightKind) -> egui::Color32 {
    match kind {
        HighlightKind::Match => match_bg(),
        HighlightKind::Focused => focused_bg(),
    }
}

/// Wrap a fixed label in a coloured frame if `kind` is `Some`. The returned
/// response is the inner label's response (so callers can use it for tooltips
/// or sizing checks). When `scroll_pending` is set and the kind is `Focused`
/// the response is also scrolled into view.
pub fn highlight_label(
    ui: &mut egui::Ui,
    text: &str,
    kind: Option<HighlightKind>,
    scroll_pending: bool,
) -> egui::Response {
    match kind {
        None => ui.label(text),
        Some(k) => {
            let resp = egui::Frame::default()
                .fill(highlight_fill(k))
                .inner_margin(egui::Margin::symmetric(3, 0))
                .corner_radius(2)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(egui::Color32::BLACK))
                })
                .inner;
            if k == HighlightKind::Focused && scroll_pending {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
            resp
        }
    }
}

/// Like [`highlight_label`] but for `ui.heading()`. We avoid changing the text
/// colour so the heading still reads as "section header", just with a tinted
/// background.
pub fn highlight_heading(
    ui: &mut egui::Ui,
    text: &str,
    kind: Option<HighlightKind>,
    scroll_pending: bool,
) -> egui::Response {
    match kind {
        None => ui.heading(text),
        Some(k) => {
            let resp = egui::Frame::default()
                .fill(highlight_fill(k))
                .inner_margin(egui::Margin::symmetric(4, 0))
                .corner_radius(2)
                .show(ui, |ui| ui.heading(text))
                .inner;
            if k == HighlightKind::Focused && scroll_pending {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
            resp
        }
    }
}

/// Options for the highlight-aware single-line text edit.
pub struct TextEditOpts<'a> {
    pub desired_width: f32,
    pub hint: Option<&'a str>,
}

impl Default for TextEditOpts<'_> {
    fn default() -> Self {
        Self {
            desired_width: 240.0,
            hint: None,
        }
    }
}

fn build_layout_job(
    text: &str,
    query_lc: &str,
    ui: &egui::Ui,
    wrap_width: f32,
    is_focused: bool,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, LayoutSection, TextFormat};
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let bg = if is_focused { focused_bg() } else { match_bg() };
    let fg = egui::Color32::BLACK;
    let mut sections: Vec<LayoutSection> = Vec::new();
    let textlc = text.to_lowercase();

    let mut cursor = 0usize;
    if !query_lc.is_empty() && !textlc.is_empty() && textlc.len() == text.len() {
        // Byte positions only line up when lowercasing is one-byte-per-byte
        // (true for ASCII). Skip substring highlighting otherwise; the
        // outer container still flags the match.
        while let Some(rel) = textlc[cursor..].find(query_lc) {
            let start = cursor + rel;
            let end = start + query_lc.len();
            if start > cursor {
                sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: cursor..start,
                    format: TextFormat {
                        font_id: font_id.clone(),
                        color: text_color,
                        ..Default::default()
                    },
                });
            }
            sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: start..end,
                format: TextFormat {
                    font_id: font_id.clone(),
                    color: fg,
                    background: bg,
                    ..Default::default()
                },
            });
            cursor = end;
            if query_lc.is_empty() {
                break;
            }
        }
    }
    if cursor < text.len() {
        sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: cursor..text.len(),
            format: TextFormat {
                font_id: font_id.clone(),
                color: text_color,
                ..Default::default()
            },
        });
    }
    let mut job = LayoutJob {
        text: text.to_string(),
        sections,
        ..Default::default()
    };
    job.wrap.max_width = wrap_width;
    job
}

/// Single-line text edit that highlights matching substrings when a query is
/// set. `highlight` controls whole-widget focus framing + scroll-to-me.
pub fn highlighted_singleline(
    ui: &mut egui::Ui,
    value: &mut String,
    query: &str,
    highlight: Option<HighlightKind>,
    opts: TextEditOpts<'_>,
    scroll_pending: bool,
) -> egui::Response {
    let qlc = query.trim().to_lowercase();
    let is_focused_match = highlight == Some(HighlightKind::Focused);
    let resp = if qlc.is_empty() {
        let mut te = egui::TextEdit::singleline(value).desired_width(opts.desired_width);
        if let Some(h) = opts.hint {
            te = te.hint_text(h);
        }
        ui.add(te)
    } else {
        let qlc_for_closure = qlc.clone();
        let mut layouter = move |ui: &egui::Ui,
                                 text: &dyn egui::TextBuffer,
                                 wrap_width: f32|
              -> Arc<egui::Galley> {
            let job = build_layout_job(
                text.as_str(),
                &qlc_for_closure,
                ui,
                wrap_width,
                is_focused_match,
            );
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let mut te = egui::TextEdit::singleline(value)
            .desired_width(opts.desired_width)
            .layouter(&mut layouter);
        if let Some(h) = opts.hint {
            te = te.hint_text(h);
        }
        ui.add(te)
    };
    if is_focused_match && scroll_pending {
        resp.scroll_to_me(Some(egui::Align::Center));
    }
    resp
}

pub struct TextAreaOpts<'a> {
    pub desired_width: f32,
    pub desired_rows: usize,
    pub hint: Option<&'a str>,
}

impl Default for TextAreaOpts<'_> {
    fn default() -> Self {
        Self {
            desired_width: f32::INFINITY,
            desired_rows: 3,
            hint: None,
        }
    }
}

pub fn highlighted_multiline(
    ui: &mut egui::Ui,
    value: &mut String,
    query: &str,
    highlight: Option<HighlightKind>,
    opts: TextAreaOpts<'_>,
    scroll_pending: bool,
) -> egui::Response {
    let qlc = query.trim().to_lowercase();
    let is_focused_match = highlight == Some(HighlightKind::Focused);
    let resp = if qlc.is_empty() {
        let mut te = egui::TextEdit::multiline(value)
            .desired_width(opts.desired_width)
            .desired_rows(opts.desired_rows);
        if let Some(h) = opts.hint {
            te = te.hint_text(h);
        }
        ui.add(te)
    } else {
        let qlc_for_closure = qlc.clone();
        let mut layouter = move |ui: &egui::Ui,
                                 text: &dyn egui::TextBuffer,
                                 wrap_width: f32|
              -> Arc<egui::Galley> {
            let job = build_layout_job(
                text.as_str(),
                &qlc_for_closure,
                ui,
                wrap_width,
                is_focused_match,
            );
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let mut te = egui::TextEdit::multiline(value)
            .desired_width(opts.desired_width)
            .desired_rows(opts.desired_rows)
            .layouter(&mut layouter);
        if let Some(h) = opts.hint {
            te = te.hint_text(h);
        }
        ui.add(te)
    };
    if is_focused_match && scroll_pending {
        resp.scroll_to_me(Some(egui::Align::Center));
    }
    resp
}

// -----------------------------------------------------------------------------
// Search bar (the floating window)
// -----------------------------------------------------------------------------

/// Outcome of one frame of the search bar — actions the App should perform
/// after rendering.
#[derive(Default)]
pub struct SearchBarOutcome {
    pub close: bool,
    pub next: bool,
    pub prev: bool,
}

/// Render the floating search window. Reads/writes state via `state`. Does
/// not call `recompute` itself — the caller does that once per frame before
/// rendering sections.
pub fn render_search_bar(ctx: &egui::Context, state: &mut SearchState) -> SearchBarOutcome {
    let mut out = SearchBarOutcome::default();
    if !state.visible {
        return out;
    }
    egui::Window::new("Find")
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 48.0])
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .show(ctx, |ui| {
            egui::Frame::default()
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Find:");
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut state.query)
                                .desired_width(220.0)
                                .hint_text("search…"),
                        );
                        if state.focus_input {
                            resp.request_focus();
                            let mut te_state =
                                egui::TextEdit::load_state(ctx, resp.id).unwrap_or_default();
                            let end = state.query.chars().count();
                            te_state.cursor.set_char_range(Some(
                                egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(end),
                                ),
                            ));
                            te_state.store(ctx, resp.id);
                            state.focus_input = false;
                        }
                        // Enter / Shift+Enter while the search input has focus
                        // steps through results. singleline TextEdit reports
                        // Enter via lost_focus + key_pressed; re-request focus
                        // so repeated taps keep advancing.
                        if resp.lost_focus()
                            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            if ctx.input(|i| i.modifiers.shift) {
                                out.prev = true;
                            } else {
                                out.next = true;
                            }
                            resp.request_focus();
                        }
                        let count = state.results.len();
                        let shown = if count == 0 {
                            "0/0".to_string()
                        } else {
                            format!("{}/{}", state.current_index + 1, count)
                        };
                        ui.label(shown);
                        if ui
                            .add_enabled(count > 0, egui::Button::new("‹"))
                            .on_hover_text("Previous (Shift+Enter)")
                            .clicked()
                        {
                            out.prev = true;
                        }
                        if ui
                            .add_enabled(count > 0, egui::Button::new("›"))
                            .on_hover_text("Next (Enter)")
                            .clicked()
                        {
                            out.next = true;
                        }
                        if ui.button("✕").on_hover_text("Close (Esc)").clicked() {
                            out.close = true;
                        }
                    });
                });
        });
    out
}
