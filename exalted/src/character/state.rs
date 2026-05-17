use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotePool {
    Personal,
    Peripheral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoteCommitment {
    pub name: String,
    pub pool: MotePool,
    pub motes: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthDamage {
    pub bashing: u8,
    pub lethal: u8,
    pub aggravated: u8,
}

impl HealthDamage {
    pub fn total(&self) -> u8 {
        self.bashing + self.lethal + self.aggravated
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolState {
    #[serde(default)]
    pub personal_motes_spent: u16,
    #[serde(default)]
    pub peripheral_motes_spent: u16,
    #[serde(default)]
    pub committed_motes: Vec<MoteCommitment>,
    #[serde(default)]
    pub willpower_temporary: u8,
    #[serde(default)]
    pub health_damage: HealthDamage,
}

impl PoolState {
    pub fn committed(&self, pool: MotePool) -> u16 {
        self.committed_motes
            .iter()
            .filter(|c| c.pool == pool)
            .map(|c| c.motes)
            .sum()
    }
}
