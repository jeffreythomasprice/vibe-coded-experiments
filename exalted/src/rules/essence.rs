use crate::character::{Character, MotePool, VirtueKind};
use crate::error::{ValidationError, ValidationReport};

/// Personal Essence max: (Essence × 3) + Willpower.
pub fn personal_essence_max(character: &Character) -> u16 {
    character.essence_dots() as u16 * 3 + character.willpower_dots() as u16
}

/// Peripheral Essence max: (Essence × 7) + Willpower + ΣVirtues.
pub fn peripheral_essence_max(character: &Character) -> u16 {
    let virtue_sum: u16 = VirtueKind::ALL
        .iter()
        .map(|v| character.virtue(*v) as u16)
        .sum();
    character.essence_dots() as u16 * 7 + character.willpower_dots() as u16 + virtue_sum
}

/// Personal motes currently available, after spend and commitment.
pub fn essence_personal_available(character: &Character) -> i32 {
    personal_essence_max(character) as i32
        - character.pool_state.personal_motes_spent as i32
        - character.pool_state.committed(MotePool::Personal) as i32
}

/// Peripheral motes currently available, after spend and commitment.
pub fn essence_peripheral_available(character: &Character) -> i32 {
    peripheral_essence_max(character) as i32
        - character.pool_state.peripheral_motes_spent as i32
        - character.pool_state.committed(MotePool::Peripheral) as i32
}

/// Runtime check on the per-session PoolState: ensures temporary Willpower
/// never exceeds permanent Willpower minus any permanently-spent points, and
/// that mote spends/commitments don't exceed their respective pools.
pub fn validate_pool_state(character: &Character) -> ValidationReport {
    let mut report = ValidationReport::new();

    let permanent = character.willpower_dots();
    let spent = character.pool_state.willpower_permanent_spent;
    if spent > permanent {
        report.push(ValidationError::WillpowerPermanentOverspent {
            spent,
            permanent,
        });
    }
    let available = permanent.saturating_sub(spent);
    if character.pool_state.willpower_temporary > available {
        report.push(ValidationError::WillpowerTemporaryOverAvailable {
            temporary: character.pool_state.willpower_temporary,
            available,
        });
    }

    if essence_personal_available(character) < 0 {
        report.push(ValidationError::PersonalEssenceOverspent {
            max: personal_essence_max(character),
            spent: character.pool_state.personal_motes_spent,
            committed: character.pool_state.committed(MotePool::Personal),
        });
    }
    if essence_peripheral_available(character) < 0 {
        report.push(ValidationError::PeripheralEssenceOverspent {
            max: peripheral_essence_max(character),
            spent: character.pool_state.peripheral_motes_spent,
            committed: character.pool_state.committed(MotePool::Peripheral),
        });
    }

    report
}
