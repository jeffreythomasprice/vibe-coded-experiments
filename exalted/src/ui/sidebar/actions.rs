//! Dice Actions panel body.
//!
//! Shows buttons for the obvious table-side rolls: the currently selected
//! attribute + ability, learned Excellencies for that ability, a few common
//! combat actions, and one button per equipped weapon. Each button's label
//! spells out the math; clicking the button rolls the dice, spends any
//! motes, and appends to the Dice Log.

use chrono::Local;

use crate::character::{AbilityKind, AttributeKind, Character, MotePool};
use crate::render::names::{ability_name, attr_name};
use crate::rules::database::prereq::{KnownExcellencies, known_excellencies};
use crate::rules::{
    derived, essence_peripheral_available, essence_personal_available, health, join_battle,
    reroll_failures, roll_pool,
};
use crate::ui::persisted::PanelItem;
use crate::ui::sidebar::dice_log::DiceLogEntry;
use crate::ui::state::AppState;

/// What clicking a roll button asks the panel to do. Collected into a
/// `Vec` during the render pass, applied after — so we don't need to
/// borrow `state` mutably mid-loop.
struct RollSpec {
    label: String,
    formula: String,
    /// Dice to actually roll (already includes any bonus dice from
    /// excellencies and any wound-penalty subtraction).
    pool: u8,
    /// Flat successes added on top of the dice result (Second Excellency).
    bonus_successes: u8,
    /// Reroll failed dice once after the initial roll (Third Excellency).
    reroll: bool,
    motes_spent: u16,
    mote_pool: Option<MotePool>,
}

pub fn render_body(ui: &mut egui::Ui, state: &mut AppState) {
    let wound = health::wound_penalty(&state.character).max(0) as u8;
    let mut pending: Vec<RollSpec> = Vec::new();

    // 1. Selection roll — always shown.
    selection_section(ui, state, wound, &mut pending);

    // 2. Excellencies for the selected ability (only those actually learned).
    if let Some(ability) = state.selection.ability {
        let known = known_excellencies(&state.character, ability);
        if known.any() {
            ui.add_space(6.0);
            excellency_section(ui, state, ability, wound, known, &mut pending);
        }
    }

    // 3. Common combat actions — always shown, no selection required.
    ui.add_space(6.0);
    combat_section(ui, &state.character, wound, &mut pending);

    // 4. Weapon attacks — one per equipped weapon.
    if !state.character.equipment.weapons.is_empty() {
        ui.add_space(6.0);
        weapons_section(ui, &state.character, wound, &mut pending);
    }

    for spec in pending {
        perform_roll(state, spec);
    }
}

fn selection_section(ui: &mut egui::Ui, state: &AppState, wound: u8, pending: &mut Vec<RollSpec>) {
    let header_text = match (state.selection.attribute, state.selection.ability) {
        (None, None) => "Selection (click an attribute and an ability)".to_string(),
        _ => "Selection".to_string(),
    };
    ui.label(egui::RichText::new(header_text).strong());

    let (label, base_pool, formula) = selection_label_and_pool(
        &state.character,
        state.selection.attribute,
        state.selection.ability,
    );
    let pool = base_pool.saturating_sub(wound);
    let formula_with_wound = wound_suffix(&formula, base_pool, wound);

    let any_selected = state.selection.attribute.is_some() || state.selection.ability.is_some();
    let btn = egui::Button::new(format!("Roll: {} = {}d", formula_with_wound, pool));
    let resp = ui.add_enabled(any_selected, btn);
    let resp = resp.on_disabled_hover_text("Click an attribute or ability to enable.");
    if resp.clicked() {
        pending.push(RollSpec {
            label,
            formula: formula_with_wound,
            pool,
            bonus_successes: 0,
            reroll: false,
            motes_spent: 0,
            mote_pool: None,
        });
    }
}

/// Build `(label, base_pool, formula)` for the selection roll. `base_pool`
/// is *before* wound subtraction; `formula` is the pre-wound math string.
fn selection_label_and_pool(
    character: &Character,
    attr: Option<AttributeKind>,
    ability: Option<AbilityKind>,
) -> (String, u8, String) {
    let attr_val = attr.map(|a| character.attribute(a)).unwrap_or(0);
    let abil_val = ability.map(|a| character.ability(a)).unwrap_or(0);
    let label = match (attr, ability) {
        (Some(a), Some(b)) => format!("{} + {}", attr_name(a), ability_name(b)),
        (Some(a), None) => attr_name(a).to_string(),
        (None, Some(b)) => ability_name(b).to_string(),
        (None, None) => "Selection".to_string(),
    };
    let formula = match (attr, ability) {
        (Some(a), Some(b)) => format!(
            "{} {} + {} {}",
            attr_name(a),
            attr_val,
            ability_name(b),
            abil_val
        ),
        (Some(a), None) => format!("{} {} + (no ability) 0", attr_name(a), attr_val),
        (None, Some(b)) => format!("(no attribute) 0 + {} {}", ability_name(b), abil_val),
        (None, None) => "(nothing selected)".to_string(),
    };
    (label, attr_val + abil_val, formula)
}

fn wound_suffix(formula: &str, base_pool: u8, wound: u8) -> String {
    if wound > 0 {
        format!(
            "{} (−{} wound) = {}",
            formula,
            wound,
            base_pool.saturating_sub(wound)
        )
    } else {
        format!("{} = {}", formula, base_pool)
    }
}

fn excellency_section(
    ui: &mut egui::Ui,
    state: &AppState,
    ability: AbilityKind,
    wound: u8,
    known: KnownExcellencies,
    pending: &mut Vec<RollSpec>,
) {
    ui.label(egui::RichText::new(format!("{} Excellencies", ability_name(ability))).strong());

    let attr = state.selection.attribute;
    let attr_val = attr.map(|a| state.character.attribute(a)).unwrap_or(0);
    let abil_val = state.character.ability(ability);
    let base_pool = attr_val + abil_val;
    let base_pool_after_wound = base_pool.saturating_sub(wound);
    let p_avail = essence_personal_available(&state.character).max(0) as u16;
    let pp_avail = essence_peripheral_available(&state.character).max(0) as u16;

    let base_formula = match attr {
        Some(a) => format!(
            "{} {} + {} {}",
            attr_name(a),
            attr_val,
            ability_name(ability),
            abil_val
        ),
        None => format!("(no attribute) 0 + {} {}", ability_name(ability), abil_val),
    };

    let attr_for_label = attr;

    if known.first {
        first_excellency_row(
            ui,
            state,
            ability,
            attr_for_label,
            abil_val,
            base_pool_after_wound,
            wound,
            &base_formula,
            p_avail,
            pp_avail,
            pending,
        );
    }
    if known.second {
        second_excellency_row(
            ui,
            state,
            ability,
            attr_for_label,
            abil_val,
            base_pool_after_wound,
            wound,
            &base_formula,
            base_pool,
            p_avail,
            pp_avail,
            pending,
        );
    }
    if known.third {
        third_excellency_row(
            ui,
            state,
            ability,
            base_pool_after_wound,
            wound,
            &base_formula,
            base_pool,
            p_avail,
            pp_avail,
            pending,
        );
    }
    if known.infinite_mastery {
        ui.add_enabled(
            false,
            egui::Button::new(format!(
                "Infinite {} Mastery — (unimplemented)",
                ability_name(ability)
            )),
        );
    }
    if known.essence_flow {
        ui.add_enabled(
            false,
            egui::Button::new(format!(
                "{} Essence Flow — (unimplemented)",
                ability_name(ability)
            )),
        );
    }
}

/// Per-row UI state for the First/Second Excellencies (how many dice/successes
/// to buy, and which pool to spend from). Stored in egui memory keyed by
/// `(ability, excellency-tier)`, so it survives frame-to-frame but not across
/// app restarts.
#[derive(Clone, Copy)]
struct ExcellencyState {
    amount: u8,
    pool: MotePool,
}

impl Default for ExcellencyState {
    fn default() -> Self {
        Self {
            amount: 1,
            pool: MotePool::Personal,
        }
    }
}

fn excellency_state(ui: &egui::Ui, salt: &str, ability: AbilityKind) -> ExcellencyState {
    let id = egui::Id::new(("exc", salt, ability as usize));
    ui.ctx()
        .data_mut(|d| d.get_temp::<ExcellencyState>(id).unwrap_or_default())
}

fn set_excellency_state(ui: &egui::Ui, salt: &str, ability: AbilityKind, st: ExcellencyState) {
    let id = egui::Id::new(("exc", salt, ability as usize));
    ui.ctx().data_mut(|d| d.insert_temp(id, st));
}

fn pool_toggle(ui: &mut egui::Ui, salt: &str, ability: AbilityKind, current: MotePool) -> MotePool {
    let mut chosen = current;
    egui::ComboBox::from_id_salt(("exc-pool", salt, ability as usize))
        .selected_text(match chosen {
            MotePool::Personal => "Personal",
            MotePool::Peripheral => "Peripheral",
        })
        .width(96.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, MotePool::Personal, "Personal");
            ui.selectable_value(&mut chosen, MotePool::Peripheral, "Peripheral");
        });
    chosen
}

#[allow(clippy::too_many_arguments)]
fn first_excellency_row(
    ui: &mut egui::Ui,
    _state: &AppState,
    ability: AbilityKind,
    attr: Option<AttributeKind>,
    abil_val: u8,
    base_pool_after_wound: u8,
    wound: u8,
    base_formula: &str,
    p_avail: u16,
    pp_avail: u16,
    pending: &mut Vec<RollSpec>,
) {
    let mut st = excellency_state(ui, "first", ability);
    // Cap: ability rating dice; further capped by motes available.
    let avail = match st.pool {
        MotePool::Personal => p_avail,
        MotePool::Peripheral => pp_avail,
    };
    let cap = (abil_val as u16).min(avail).min(u8::MAX as u16) as u8;
    if st.amount > cap {
        st.amount = cap;
    }
    if st.amount == 0 {
        st.amount = cap.min(1);
    }

    ui.horizontal(|ui| {
        ui.label("First Excellency:");
        let mut amount = st.amount as u32;
        let drag = egui::DragValue::new(&mut amount)
            .range(0u32..=cap as u32)
            .speed(0.2)
            .suffix("d");
        if ui.add(drag).changed() {
            st.amount = (amount as u8).min(cap);
        }
        st.pool = pool_toggle(ui, "first", ability, st.pool);
    });
    set_excellency_state(ui, "first", ability, st);

    let cost = st.amount as u16;
    let total_dice = base_pool_after_wound.saturating_add(st.amount);
    let formula = format!(
        "{}{} + {} (1st Exc) = {}",
        base_formula,
        wound_phrase(wound),
        st.amount,
        total_dice
    );
    let label = match attr {
        Some(a) => format!("{} + {} (1st Exc)", attr_name(a), ability_name(ability)),
        None => format!("{} (1st Exc)", ability_name(ability)),
    };

    let enabled = st.amount > 0 && cost > 0 && avail >= cost;
    let btn = egui::Button::new(format!("Roll +{}d for {}m {:?}", st.amount, cost, st.pool));
    if ui
        .add_enabled(enabled, btn)
        .on_disabled_hover_text("Not enough motes in the selected pool, or no dice selected.")
        .clicked()
    {
        pending.push(RollSpec {
            label,
            formula,
            pool: total_dice,
            bonus_successes: 0,
            reroll: false,
            motes_spent: cost,
            mote_pool: Some(st.pool),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn second_excellency_row(
    ui: &mut egui::Ui,
    _state: &AppState,
    ability: AbilityKind,
    attr: Option<AttributeKind>,
    abil_val: u8,
    base_pool_after_wound: u8,
    wound: u8,
    base_formula: &str,
    _base_pool: u8,
    p_avail: u16,
    pp_avail: u16,
    pending: &mut Vec<RollSpec>,
) {
    let mut st = excellency_state(ui, "second", ability);
    // Cap: floor(ability / 2) successes; further capped by motes available / 2.
    let success_cap = (abil_val / 2) as u16;
    let avail = match st.pool {
        MotePool::Personal => p_avail,
        MotePool::Peripheral => pp_avail,
    };
    let cap = success_cap.min(avail / 2).min(u8::MAX as u16) as u8;
    if st.amount > cap {
        st.amount = cap;
    }
    if st.amount == 0 {
        st.amount = cap.min(1);
    }

    ui.horizontal(|ui| {
        ui.label("Second Excellency:");
        let mut amount = st.amount as u32;
        let drag = egui::DragValue::new(&mut amount)
            .range(0u32..=cap as u32)
            .speed(0.2)
            .suffix(" succ");
        if ui.add(drag).changed() {
            st.amount = (amount as u8).min(cap);
        }
        st.pool = pool_toggle(ui, "second", ability, st.pool);
    });
    set_excellency_state(ui, "second", ability, st);

    let cost = (st.amount as u16) * 2;
    let formula = format!(
        "{}{} → roll {}d, then +{} successes (2nd Exc)",
        base_formula,
        wound_phrase(wound),
        base_pool_after_wound,
        st.amount
    );
    let label = match attr {
        Some(a) => format!("{} + {} (2nd Exc)", attr_name(a), ability_name(ability)),
        None => format!("{} (2nd Exc)", ability_name(ability)),
    };
    let enabled = st.amount > 0 && cost > 0 && avail >= cost;
    let btn = egui::Button::new(format!(
        "Roll +{} successes for {}m {:?}",
        st.amount, cost, st.pool
    ));
    if ui
        .add_enabled(enabled, btn)
        .on_disabled_hover_text("Not enough motes in the selected pool, or no successes selected.")
        .clicked()
    {
        pending.push(RollSpec {
            label,
            formula,
            pool: base_pool_after_wound,
            bonus_successes: st.amount,
            reroll: false,
            motes_spent: cost,
            mote_pool: Some(st.pool),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn third_excellency_row(
    ui: &mut egui::Ui,
    _state: &AppState,
    ability: AbilityKind,
    base_pool_after_wound: u8,
    wound: u8,
    base_formula: &str,
    _base_pool: u8,
    p_avail: u16,
    pp_avail: u16,
    pending: &mut Vec<RollSpec>,
) {
    let mut st = excellency_state(ui, "third", ability);
    let avail = match st.pool {
        MotePool::Personal => p_avail,
        MotePool::Peripheral => pp_avail,
    };
    const COST: u16 = 4;

    ui.horizontal(|ui| {
        ui.label("Third Excellency:");
        st.pool = pool_toggle(ui, "third", ability, st.pool);
    });
    set_excellency_state(ui, "third", ability, st);

    let formula = format!(
        "{}{} = {} (3rd Exc: reroll failures once)",
        base_formula,
        wound_phrase(wound),
        base_pool_after_wound
    );
    let label = format!("{} (3rd Exc)", ability_name(ability));
    let enabled = avail >= COST;
    let btn = egui::Button::new(format!("Roll, reroll failures for 4m {:?}", st.pool));
    if ui
        .add_enabled(enabled, btn)
        .on_disabled_hover_text("Not enough motes in the selected pool.")
        .clicked()
    {
        pending.push(RollSpec {
            label,
            formula,
            pool: base_pool_after_wound,
            bonus_successes: 0,
            reroll: true,
            motes_spent: COST,
            mote_pool: Some(st.pool),
        });
    }
}

fn wound_phrase(wound: u8) -> String {
    if wound > 0 {
        format!(" (−{} wound)", wound)
    } else {
        String::new()
    }
}

fn combat_section(
    ui: &mut egui::Ui,
    character: &Character,
    wound: u8,
    pending: &mut Vec<RollSpec>,
) {
    ui.label(egui::RichText::new("Common actions").strong());

    let jb_base = join_battle(character);
    let jb_pool = jb_base.saturating_sub(wound);
    let jb_formula = format!(
        "Wits {} + Awareness {}",
        character.attribute(crate::character::AttributeKind::Wits),
        character.ability(crate::character::AbilityKind::Awareness),
    );
    let jb_label = format!("{}{} = {}", jb_formula, wound_phrase(wound), jb_pool);
    if ui.button(format!("Join Battle: {}", jb_label)).clicked() {
        pending.push(RollSpec {
            label: "Join Battle".to_string(),
            formula: jb_label,
            pool: jb_pool,
            bonus_successes: 0,
            reroll: false,
            motes_spent: 0,
            mote_pool: None,
        });
    }

    let k = derived::knockdown(character);
    let k_pool = k.resist_pool.saturating_sub(wound);
    let k_label = format!(
        "Resist Knockdown: best of (Dex+Ath, Sta+Res){} = {}",
        wound_phrase(wound),
        k_pool
    );
    if ui.button(k_label.clone()).clicked() {
        pending.push(RollSpec {
            label: "Resist Knockdown".to_string(),
            formula: k_label,
            pool: k_pool,
            bonus_successes: 0,
            reroll: false,
            motes_spent: 0,
            mote_pool: None,
        });
    }

    let s = derived::stunning(character);
    let s_pool = s.resist_pool.saturating_sub(wound);
    let s_label = format!(
        "Resist Stunning: Sta+Res{} = {}",
        wound_phrase(wound),
        s_pool
    );
    if ui.button(s_label.clone()).clicked() {
        pending.push(RollSpec {
            label: "Resist Stunning".to_string(),
            formula: s_label,
            pool: s_pool,
            bonus_successes: 0,
            reroll: false,
            motes_spent: 0,
            mote_pool: None,
        });
    }
}

fn weapons_section(
    ui: &mut egui::Ui,
    character: &Character,
    wound: u8,
    pending: &mut Vec<RollSpec>,
) {
    ui.label(egui::RichText::new("Weapon attacks").strong());

    let dex = character.attribute(crate::character::AttributeKind::Dexterity);
    for weapon in &character.equipment.weapons {
        let ability_val = character.ability(weapon.ability);
        let acc = weapon.accuracy as i16;
        let pre = dex as i16 + ability_val as i16 + acc;
        let base = pre.max(0) as u8;
        let final_pool = base.saturating_sub(wound);
        let formula = format!(
            "Dex {} + {} {}{}{} = {}",
            dex,
            ability_name(weapon.ability),
            ability_val,
            if acc >= 0 {
                format!(" + acc {}", acc)
            } else {
                format!(" − acc {}", acc.unsigned_abs())
            },
            wound_phrase(wound),
            final_pool
        );
        let label = format!("Attack: {}", weapon.name);
        if ui
            .button(format!("{} — {}d ({})", label, final_pool, formula))
            .clicked()
        {
            pending.push(RollSpec {
                label,
                formula,
                pool: final_pool,
                bonus_successes: 0,
                reroll: false,
                motes_spent: 0,
                mote_pool: None,
            });
        }
    }
}

fn perform_roll(state: &mut AppState, spec: RollSpec) {
    if let (Some(pool), n) = (spec.mote_pool, spec.motes_spent)
        && n > 0
    {
        state.character.pool_state.spend_motes(pool, n);
        state.mark_dirty();
    }
    let mut result = roll_pool(spec.pool);
    if spec.reroll {
        let mut rng = rand::rng();
        reroll_failures(&mut rng, &mut result);
    }
    state.dice_log.push(DiceLogEntry {
        label: spec.label,
        formula: spec.formula,
        rolls: result.rolls,
        dice_successes: result.successes,
        bonus_successes: spec.bonus_successes,
        botch: result.botch && spec.bonus_successes == 0,
        motes_spent: spec.motes_spent,
        mote_pool: spec.mote_pool,
        when: Local::now(),
    });
    state.ui_state.focus_panel(PanelItem::DiceLog);
}
