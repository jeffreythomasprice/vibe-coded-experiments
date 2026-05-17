use crate::character::{Character, MotePool, VirtueKind};

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
