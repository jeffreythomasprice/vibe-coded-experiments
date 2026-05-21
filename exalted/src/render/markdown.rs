//! Markdown rendering of a `Character` into a human-readable sheet.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::character::xp::total_xp_spent;
use crate::character::{
    AbilityKind, AttributeGroup, Character, CharmRef, DotSource, Equipment, Hearthstone,
    KnownLanguage, LanguageFamily, MotePool, RatedTrait, SpellRef, VirtueKind,
};
use crate::rules::database::database;
use crate::rules::derived::{knockdown, movement, stunning, EXALT_HEALING_TABLE};
use crate::rules::essence::{personal_essence_max, peripheral_essence_max};
use crate::rules::health::{health_track, wound_penalty, HealthLevelKind};

use super::names::{
    ability_name, attr_name, caste_name, group_name, intimacy_kind, source_tag,
    spell_circle_label, virtue_name,
};

pub fn character_to_markdown(c: &Character) -> String {
    let mut out = String::new();

    section_identity(c, &mut out);
    section_attributes(c, &mut out);
    section_abilities(c, &mut out);
    section_virtues(c, &mut out);
    section_willpower_essence(c, &mut out);
    section_charms(c, &mut out);
    section_spells(&c.spells, &mut out);
    section_backgrounds(c, &mut out);
    section_intimacies(c, &mut out);
    section_familiar(c, &mut out);
    section_equipment(&c.equipment, &mut out);
    section_hearthstones(&c.hearthstones, &mut out);
    section_languages(&c.languages, &mut out);
    section_health_and_pools(c, &mut out);
    section_movement_combat(c, &mut out);
    section_healing(&mut out);
    section_xp(c, &mut out);
    section_notes(&c.notes, &mut out);

    out
}

fn dots(rating: u8, max: u8) -> String {
    let filled = rating.min(max);
    let empty = max.saturating_sub(filled);
    let mut s = String::with_capacity((max as usize) * 3);
    for _ in 0..filled {
        s.push('●');
    }
    for _ in 0..empty {
        s.push('○');
    }
    s
}

fn section_identity(c: &Character, out: &mut String) {
    let id = &c.identity;
    let name = if id.name.is_empty() { "<unnamed>" } else { &id.name };
    writeln!(out, "# {}", name).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "**Caste:** {}", caste_name(c.caste)).unwrap();
    if !id.concept.is_empty() {
        writeln!(out, "**Concept:** {}", id.concept).unwrap();
    }
    if !id.motivation.is_empty() {
        writeln!(out, "**Motivation:** {}", id.motivation).unwrap();
    }
    if !id.personality.is_empty() {
        writeln!(out, "**Personality:** {}", id.personality).unwrap();
    }
    if !id.player.is_empty() {
        writeln!(out, "**Player:** {}", id.player).unwrap();
    }
    if !id.chronicle.is_empty() {
        writeln!(out, "**Chronicle:** {}", id.chronicle).unwrap();
    }
    writeln!(out).unwrap();

    let anima = &id.anima;
    if !anima.totem.is_empty() {
        writeln!(out, "## Anima").unwrap();
        writeln!(out, "- **Totem:** {}", anima.totem).unwrap();
        writeln!(out).unwrap();
    }

    let app = &id.appearance;
    let any_appearance = !app.hair.is_empty()
        || !app.eyes.is_empty()
        || !app.skin.is_empty()
        || !app.distinguishing_features.is_empty()
        || !app.homeland.is_empty()
        || !app.sex.is_empty()
        || app.age.is_some();
    if any_appearance {
        writeln!(out, "## Appearance").unwrap();
        if !app.sex.is_empty() {
            writeln!(out, "- **Sex:** {}", app.sex).unwrap();
        }
        if let Some(age) = app.age {
            writeln!(out, "- **Age:** {}", age).unwrap();
        }
        if !app.hair.is_empty() {
            writeln!(out, "- **Hair:** {}", app.hair).unwrap();
        }
        if !app.eyes.is_empty() {
            writeln!(out, "- **Eyes:** {}", app.eyes).unwrap();
        }
        if !app.skin.is_empty() {
            writeln!(out, "- **Skin:** {}", app.skin).unwrap();
        }
        if !app.homeland.is_empty() {
            writeln!(out, "- **Homeland:** {}", app.homeland).unwrap();
        }
        if !app.distinguishing_features.is_empty() {
            writeln!(
                out,
                "- **Distinguishing features:** {}",
                app.distinguishing_features
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    writeln!(out, "## Favored Abilities").unwrap();
    if c.favored_abilities.is_empty() {
        writeln!(out, "- (none)").unwrap();
    } else {
        for ab in &c.favored_abilities {
            writeln!(out, "- {}", ability_name(*ab)).unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn section_attributes(c: &Character, out: &mut String) {
    writeln!(out, "## Attributes").unwrap();
    let prio = c.attribute_priority;
    writeln!(
        out,
        "Priority: {} (primary, 8) / {} (secondary, 6) / {} (tertiary, 4)",
        group_name(prio.primary),
        group_name(prio.secondary),
        group_name(prio.tertiary)
    )
    .unwrap();
    writeln!(out).unwrap();

    writeln!(
        out,
        "| Physical | | Social | | Mental | |"
    )
    .unwrap();
    writeln!(
        out,
        "|---|---|---|---|---|---|"
    )
    .unwrap();
    let phys = AttributeGroup::Physical.members();
    let soc = AttributeGroup::Social.members();
    let men = AttributeGroup::Mental.members();
    for i in 0..3 {
        let p = phys[i];
        let s = soc[i];
        let m = men[i];
        writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} |",
            attr_name(p),
            dots(c.attribute(p), 5),
            attr_name(s),
            dots(c.attribute(s), 5),
            attr_name(m),
            dots(c.attribute(m), 5),
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn section_abilities(c: &Character, out: &mut String) {
    writeln!(out, "## Abilities").unwrap();
    writeln!(out, "| Ability | Dots | C/F | Specialties |").unwrap();
    writeln!(out, "|---|---|---|---|").unwrap();
    let mut rows: Vec<(AbilityKind, u8)> = c
        .abilities
        .iter()
        .map(|(k, t)| (*k, t.dots()))
        .collect();
    rows.sort_by_key(|(k, _)| ability_name(*k));
    for (ab, _) in &rows {
        let dots_n = c.ability(*ab);
        let cf = if c.is_caste_ability(*ab) {
            "C"
        } else if c.is_favored_ability(*ab) {
            "F"
        } else {
            ""
        };
        let specs = c
            .abilities
            .get(ab)
            .map(|t| {
                t.aggregated_specialties()
                    .into_iter()
                    .map(|(name, n)| format!("{} {}", name, n))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        writeln!(
            out,
            "| {} | {} ({}) | {} | {} |",
            ability_name(*ab),
            dots(dots_n, 5),
            dots_n,
            cf,
            specs
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn section_virtues(c: &Character, out: &mut String) {
    writeln!(out, "## Virtues").unwrap();
    writeln!(out, "| Virtue | Dots | Primary? |").unwrap();
    writeln!(out, "|---|---|---|").unwrap();
    for v in VirtueKind::ALL {
        let primary = if c.primary_virtue == Some(*v) { "★" } else { "" };
        let n = c.virtue(*v);
        writeln!(
            out,
            "| {} | {} ({}) | {} |",
            virtue_name(*v),
            dots(n, 5),
            n,
            primary
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    if let Some(flaw) = &c.virtue_flaw {
        writeln!(out, "**Virtue flaw:** {:?}", flaw).unwrap();
    }
    writeln!(
        out,
        "**Limit:** {} / {}{}",
        c.pool_state.limit,
        crate::character::PoolState::LIMIT_BREAK_THRESHOLD,
        if c.pool_state.is_at_limit_break() {
            " — **LIMIT BREAK**"
        } else {
            ""
        }
    )
    .unwrap();
    let channels: Vec<String> = VirtueKind::ALL
        .iter()
        .filter_map(|v| {
            let dots = c.virtue(*v);
            if dots == 0 {
                return None;
            }
            let remaining = c.pool_state.channels_remaining(*v, dots);
            Some(format!("{} {}/{}", virtue_name(*v), remaining, dots))
        })
        .collect();
    if !channels.is_empty() {
        writeln!(out, "**Channels remaining this story:** {}", channels.join(" · ")).unwrap();
    }
    writeln!(out).unwrap();
}

fn section_willpower_essence(c: &Character, out: &mut String) {
    writeln!(out, "## Willpower & Essence").unwrap();
    let wp = c.willpower_dots();
    writeln!(
        out,
        "- **Willpower:** {} ({})  — temp {} / permanent spent {}",
        dots(wp, 10),
        wp,
        c.pool_state.willpower_temporary,
        c.pool_state.willpower_permanent_spent
    )
    .unwrap();
    let ess = c.essence_dots();
    let pers_max = personal_essence_max(c);
    let peri_max = peripheral_essence_max(c);
    let pers_committed = c.pool_state.committed(MotePool::Personal);
    let peri_committed = c.pool_state.committed(MotePool::Peripheral);
    writeln!(out, "- **Essence:** {} ({})", dots(ess, 10), ess).unwrap();
    writeln!(
        out,
        "  - Personal: max {}, spent {}, committed {}",
        pers_max, c.pool_state.personal_motes_spent, pers_committed
    )
    .unwrap();
    writeln!(
        out,
        "  - Peripheral: max {}, spent {}, committed {}",
        peri_max, c.pool_state.peripheral_motes_spent, peri_committed
    )
    .unwrap();
    if !c.pool_state.committed_motes.is_empty() {
        writeln!(out, "  - Commitments:").unwrap();
        for cm in &c.pool_state.committed_motes {
            let pool = match cm.pool {
                MotePool::Personal => "personal",
                MotePool::Peripheral => "peripheral",
            };
            writeln!(out, "    - {} — {} motes ({})", cm.name, cm.motes, pool).unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn section_charms(c: &Character, out: &mut String) {
    writeln!(out, "## Charms ({})", c.charms.len()).unwrap();
    if c.charms.is_empty() {
        writeln!(out, "(none)").unwrap();
        writeln!(out).unwrap();
        return;
    }
    let db = database();
    let mut grouped: BTreeMap<&'static str, Vec<&CharmRef>> = BTreeMap::new();
    for charm in &c.charms {
        let label = charm.ability(db).map(ability_name).unwrap_or("Other");
        grouped.entry(label).or_default().push(charm);
    }
    for (ability, charms) in &grouped {
        writeln!(out, "### {}", ability).unwrap();
        for charm in charms {
            let mut suffix = format!(" *[{}]*", source_tag(charm.source()));
            if charm.non_solar() {
                suffix.push_str(" *(non-Solar)*");
            }
            writeln!(out, "- {}{}", charm.display_name(db), suffix).unwrap();
            if let Some(notes) = charm.notes() {
                writeln!(out, "  - {}", notes).unwrap();
            }
        }
    }
    writeln!(out).unwrap();
}

fn section_spells(spells: &[SpellRef], out: &mut String) {
    writeln!(out, "## Spells ({})", spells.len()).unwrap();
    if spells.is_empty() {
        writeln!(out, "(none)").unwrap();
        writeln!(out).unwrap();
        return;
    }
    let db = database();
    for s in spells {
        let circle_label = s.circle(db).map(spell_circle_label).unwrap_or("?");
        writeln!(
            out,
            "- **{}** ({} Circle) *[{}]*",
            s.display_name(db),
            circle_label,
            source_tag(s.source())
        )
        .unwrap();
        if let Some(notes) = s.notes() {
            writeln!(out, "  - {}", notes).unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn section_backgrounds(c: &Character, out: &mut String) {
    writeln!(out, "## Backgrounds").unwrap();
    if c.backgrounds.is_empty() {
        writeln!(out, "(none)").unwrap();
        writeln!(out).unwrap();
        return;
    }
    let db = database();
    writeln!(out, "| Background | Label | Dots | Notes |").unwrap();
    writeln!(out, "|---|---|---|---|").unwrap();
    for bg in &c.backgrounds {
        let n = bg.trait_().dots();
        writeln!(
            out,
            "| {} | {} | {} ({}) | {} |",
            bg.display_name(db),
            if bg.label().is_empty() { "—" } else { bg.label() },
            dots(n, 5),
            n,
            bg.notes().unwrap_or(""),
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn section_intimacies(c: &Character, out: &mut String) {
    writeln!(out, "## Intimacies ({})", c.intimacies.len()).unwrap();
    if c.intimacies.is_empty() {
        writeln!(out, "(none)").unwrap();
        writeln!(out).unwrap();
        return;
    }
    for i in &c.intimacies {
        writeln!(
            out,
            "- **{}** ({}) — {} ({}/10) *[{}]*",
            i.description,
            intimacy_kind(i.kind),
            dots(i.rating, 10),
            i.rating,
            source_tag(i.source)
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn section_familiar(c: &Character, out: &mut String) {
    let Some(fam) = &c.familiar else {
        return;
    };
    writeln!(out, "## Familiar").unwrap();
    let name = if fam.name.is_empty() { "<unnamed>" } else { &fam.name };
    writeln!(out, "- Name: {}", name).unwrap();
    let d = &fam.health_damage;
    writeln!(
        out,
        "- Damage: {}B / {}L / {}A ({} / 30)",
        d.bashing,
        d.lethal,
        d.aggravated,
        d.total()
    )
    .unwrap();
    writeln!(out).unwrap();
}

fn section_equipment(eq: &Equipment, out: &mut String) {
    writeln!(out, "## Equipment").unwrap();

    writeln!(out, "### Weapons").unwrap();
    if eq.weapons.is_empty() {
        writeln!(out, "(none)").unwrap();
    } else {
        writeln!(
            out,
            "| Name | Ability | Speed | Acc | Dmg | Type | Def | Rate | Range | Tags |"
        )
        .unwrap();
        writeln!(
            out,
            "|---|---|---|---|---|---|---|---|---|---|"
        )
        .unwrap();
        for w in &eq.weapons {
            let dmg_type = match w.damage_type {
                crate::character::equipment::DamageType::Bashing => "B",
                crate::character::equipment::DamageType::Lethal => "L",
                crate::character::equipment::DamageType::Aggravated => "A",
            };
            let range = w
                .range_yards
                .map(|r| format!("{}y", r))
                .unwrap_or_else(|| "—".to_string());
            let tags = if w.tags.is_empty() {
                "—".to_string()
            } else {
                w.tags.join(", ")
            };
            let mut name = w.name.clone();
            if let Some(artifact) = &w.artifact_name {
                name = format!("{} *(artifact: {})*", name, artifact);
            }
            writeln!(
                out,
                "| {} | {} | {:+} | {:+} | {:+} | {} | {:+} | {} | {} | {} |",
                name,
                ability_name(w.ability),
                w.speed,
                w.accuracy,
                w.damage,
                dmg_type,
                w.defense,
                w.rate,
                range,
                tags
            )
            .unwrap();
        }
    }
    writeln!(out).unwrap();

    writeln!(out, "### Armor").unwrap();
    if let Some(a) = &eq.armor {
        writeln!(
            out,
            "**{}** — soak B/L/A: {}/{}/{}; hardness B/L: {}/{}; mobility −{}; fatigue {}",
            a.name,
            a.soak_bashing,
            a.soak_lethal,
            a.soak_aggravated,
            a.hardness_bashing,
            a.hardness_lethal,
            a.mobility_penalty,
            a.fatigue
        )
        .unwrap();
        if let Some(m) = a.attunement_motes {
            writeln!(out, "- Attunement: {} motes", m).unwrap();
        }
        if let Some(artifact) = &a.artifact_name {
            writeln!(out, "- Artifact: {}", artifact).unwrap();
        }
    } else {
        writeln!(out, "(none)").unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "### Artifacts").unwrap();
    if eq.artifacts.is_empty() {
        writeln!(out, "(none)").unwrap();
    } else {
        for a in &eq.artifacts {
            writeln!(
                out,
                "- **{}** (rating {}, attunement {} motes, {} sockets)",
                a.name, a.rating, a.attunement_motes, a.hearthstone_sockets
            )
            .unwrap();
            if !a.description.is_empty() {
                writeln!(out, "  - {}", a.description).unwrap();
            }
            if !a.socketed_hearthstones.is_empty() {
                writeln!(
                    out,
                    "  - Socketed: {}",
                    a.socketed_hearthstones.join(", ")
                )
                .unwrap();
            }
        }
    }
    writeln!(out).unwrap();

    writeln!(out, "### Other Possessions").unwrap();
    if eq.other.is_empty() {
        writeln!(out, "(none)").unwrap();
    } else {
        for p in &eq.other {
            let loc = match &p.secondary_location {
                Some(s) => format!("{} / {}", p.primary_location, s),
                None => p.primary_location.clone(),
            };
            writeln!(out, "- {} ({})", p.name, loc).unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn section_hearthstones(stones: &[Hearthstone], out: &mut String) {
    writeln!(out, "## Hearthstones").unwrap();
    if stones.is_empty() {
        writeln!(out, "(none)").unwrap();
        writeln!(out).unwrap();
        return;
    }
    for s in stones {
        let aspect = if s.aspect.is_empty() {
            String::new()
        } else {
            format!(", {}", s.aspect)
        };
        writeln!(out, "- **{}** (level {}{})", s.name, s.level, aspect).unwrap();
        if !s.description.is_empty() {
            writeln!(out, "  - {}", s.description).unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn section_languages(langs: &[KnownLanguage], out: &mut String) {
    writeln!(out, "## Languages").unwrap();
    if langs.is_empty() {
        writeln!(out, "(none)").unwrap();
        writeln!(out).unwrap();
        return;
    }
    for l in langs {
        let native = if l.native { " *(native)*" } else { "" };
        let dialect = l
            .dialect_specialty
            .as_ref()
            .map(|d| format!(" — dialect: {}", d))
            .unwrap_or_default();
        let family = match &l.family {
            LanguageFamily::TribalTongue(name) => format!("Tribal Tongue: {}", name),
            other => format!("{:?}", other),
        };
        writeln!(out, "- {}{}{}", family, dialect, native).unwrap();
    }
    writeln!(out).unwrap();
}

fn section_health_and_pools(c: &Character, out: &mut String) {
    writeln!(out, "## Health").unwrap();
    let track = health_track(c);
    let dmg = &c.pool_state.health_damage;
    let total_dmg = dmg.total() as usize;
    writeln!(
        out,
        "Damage — Bashing: {}, Lethal: {}, Aggravated: {} (total {})",
        dmg.bashing, dmg.lethal, dmg.aggravated, total_dmg
    )
    .unwrap();
    writeln!(out, "Wound penalty: {}", wound_penalty(c)).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| # | Level | Penalty | Filled |").unwrap();
    writeln!(out, "|---|---|---|---|").unwrap();
    for (i, slot) in track.iter().enumerate() {
        let filled = if i < total_dmg { "✗" } else { "□" };
        let kind = match slot.kind {
            HealthLevelKind::Wound => "Wound",
            HealthLevelKind::Incap => "Incap",
            HealthLevelKind::Dying => "Dying",
        };
        writeln!(
            out,
            "| {} | {} ({}) | {:+} | {} |",
            i + 1,
            slot.label,
            kind,
            slot.penalty,
            filled
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn section_movement_combat(c: &Character, out: &mut String) {
    let m = movement(c);
    let k = knockdown(c);
    let s = stunning(c);
    writeln!(out, "## Movement").unwrap();
    writeln!(out, "- **Move:** {} yd/tick (reflexive)", m.move_).unwrap();
    writeln!(out, "- **Dash:** {} yd/tick (Speed 3, −2 DV, cannot parry)", m.dash).unwrap();
    writeln!(
        out,
        "- **Jump:** {} yd vertical / {} yd horizontal",
        m.jump_vertical, m.jump_horizontal
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## Knockdown & Stunning").unwrap();
    writeln!(
        out,
        "- **Knockdown:** triggers when raw damage > {} (Sta + Resistance). \
         Resist: {} dice vs diff 2.",
        k.threshold, k.resist_pool
    )
    .unwrap();
    writeln!(
        out,
        "- **Stunning:** triggers when one blow inflicts > {} HLs (Stamina). \
         Resist: {} dice (Sta + Resistance) vs diff (HLs − Sta).",
        s.threshold, s.resist_pool
    )
    .unwrap();
    writeln!(out).unwrap();
}

fn section_healing(out: &mut String) {
    writeln!(out, "## Healing Rates (Exalted)").unwrap();
    writeln!(out, "| Tier | Bashing | Lethal (rest) | Lethal (active) |").unwrap();
    writeln!(out, "|---|---|---|---|").unwrap();
    for row in EXALT_HEALING_TABLE {
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            row.tier, row.bashing, row.lethal_rest, row.lethal_active
        )
        .unwrap();
    }
    writeln!(out, "Aggravated damage heals at the lethal rate (p.151).").unwrap();
    writeln!(out).unwrap();
}

fn section_xp(c: &Character, out: &mut String) {
    writeln!(out, "## Experience").unwrap();
    let spent = total_xp_spent(c);
    writeln!(out, "- Earned: {}", c.xp_earned).unwrap();
    writeln!(out, "- Spent: {}", spent).unwrap();
    writeln!(out, "- Banked (declared): {}", c.xp_banked).unwrap();
    writeln!(out).unwrap();

    let purchases = collect_xp_purchases(c);
    if !purchases.is_empty() {
        writeln!(out, "### XP Purchases").unwrap();
        writeln!(out, "| Trait | Cost |").unwrap();
        writeln!(out, "|---|---|").unwrap();
        for (label, cost) in purchases {
            writeln!(out, "| {} | {} |", label, cost).unwrap();
        }
        writeln!(out).unwrap();
    }
}

fn collect_xp_purchases(c: &Character) -> Vec<(String, u32)> {
    let mut rows = Vec::new();
    for (kind, t) in &c.attributes {
        push_xp_dot_rows(&mut rows, attr_name(*kind), t);
        push_xp_spec_rows(&mut rows, attr_name(*kind), t);
    }
    for (kind, t) in &c.abilities {
        push_xp_dot_rows(&mut rows, ability_name(*kind), t);
        push_xp_spec_rows(&mut rows, ability_name(*kind), t);
    }
    for (kind, t) in &c.virtues {
        push_xp_dot_rows(&mut rows, virtue_name(*kind), t);
    }
    push_xp_dot_rows(&mut rows, "Willpower", &c.willpower);
    push_xp_dot_rows(&mut rows, "Essence", &c.essence);
    let db = database();
    for bg in &c.backgrounds {
        let label = if bg.label().is_empty() {
            bg.display_name(db).to_string()
        } else {
            format!("{} ({})", bg.display_name(db), bg.label())
        };
        push_xp_dot_rows(&mut rows, &label, bg.trait_());
    }
    for charm in &c.charms {
        if let DotSource::Xp { spent } = charm.source() {
            rows.push((format!("Charm: {}", charm.display_name(db)), spent));
        }
    }
    for spell in &c.spells {
        if let DotSource::Xp { spent } = spell.source() {
            rows.push((format!("Spell: {}", spell.display_name(db)), spent));
        }
    }
    for intimacy in &c.intimacies {
        if let DotSource::Xp { spent } = intimacy.source {
            rows.push((format!("Intimacy: {}", intimacy.description), spent));
        }
    }
    rows
}

fn push_xp_dot_rows(rows: &mut Vec<(String, u32)>, label: &str, t: &RatedTrait) {
    for p in &t.purchases {
        if let DotSource::Xp { spent } = p.source {
            rows.push((format!("{} +1 dot", label), spent));
        }
    }
}

fn push_xp_spec_rows(rows: &mut Vec<(String, u32)>, label: &str, t: &RatedTrait) {
    for s in &t.specialties {
        if let DotSource::Xp { spent } = s.source {
            rows.push((format!("{} specialty: {}", label, s.name), spent));
        }
    }
}

fn section_notes(notes: &BTreeMap<String, String>, out: &mut String) {
    if notes.is_empty() {
        return;
    }
    writeln!(out, "## Notes").unwrap();
    for (k, v) in notes {
        writeln!(out, "### {}", k).unwrap();
        writeln!(out, "{}", v).unwrap();
        writeln!(out).unwrap();
    }
}
