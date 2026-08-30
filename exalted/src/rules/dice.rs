use crate::character::{AbilityKind, AttributeKind, Character};

/// Specialty bonus for a given ability (capped at +3 per rulebook, except
/// Linguistics which is uncapped). Returns 0 if `specialty` is `None` or
/// doesn't match any specialty the character has on that ability.
pub fn specialty_bonus(character: &Character, ability: AbilityKind, specialty: Option<&str>) -> u8 {
    match specialty {
        None => 0,
        Some(name) => character
            .abilities
            .get(&ability)
            .map(|t| {
                let matching = t.specialties.iter().filter(|s| s.name == name).count();
                if ability == AbilityKind::Linguistics {
                    matching as u8
                } else {
                    matching.min(3) as u8
                }
            })
            .unwrap_or(0),
    }
}

/// Base dice pool for an Attribute + Ability action, plus an optional
/// specialty (capped at +3 per rulebook). Does not include external
/// modifiers (stunt dice, charms, etc.).
///
/// For `AbilityKind::Craft` the rating is the character's *best* craft (see
/// [`Character::ability`]); this signature can't address a specific craft
/// focus, so per-focus craft specialties don't apply here. A dedicated
/// per-focus helper can be added if precise craft pools are ever needed.
pub fn dice_pool(
    character: &Character,
    attribute: AttributeKind,
    ability: AbilityKind,
    specialty: Option<&str>,
) -> u8 {
    let attr = character.attribute(attribute);
    let abil = character.ability(ability);
    let spec = specialty_bonus(character, ability, specialty);
    attr + abil + spec
}
