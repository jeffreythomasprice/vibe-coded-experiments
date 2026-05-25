use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::traits::VirtueKind;

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
    pub willpower_permanent_spent: u8,
    #[serde(default)]
    pub health_damage: HealthDamage,
    /// Current Limit dots. Limit Break fires automatically at
    /// [`PoolState::LIMIT_BREAK_THRESHOLD`] (p.105). Stored on PoolState
    /// because it's per-scene/per-session state, not chargen identity.
    #[serde(default)]
    pub limit: u8,
    /// Per-virtue, per-story channel counter (p.102). Once per story per
    /// dot of the Virtue. Reset at story boundaries by the player/ST.
    #[serde(default)]
    pub virtue_channels_used: BTreeMap<VirtueKind, u8>,
}

impl PoolState {
    /// Limit Break fires automatically when the Limit meter reaches this
    /// value (Exalted 2E p.105).
    pub const LIMIT_BREAK_THRESHOLD: u8 = 10;

    pub fn committed(&self, pool: MotePool) -> u16 {
        self.committed_motes
            .iter()
            .filter(|c| c.pool == pool)
            .map(|c| c.motes)
            .sum()
    }

    /// Permanent Willpower dots still available given a character's permanent
    /// Willpower rating.
    pub fn willpower_available(&self, permanent: u8) -> i32 {
        permanent as i32 - self.willpower_permanent_spent as i32
    }

    /// True once Limit has reached the break threshold (10).
    pub fn is_at_limit_break(&self) -> bool {
        self.limit >= Self::LIMIT_BREAK_THRESHOLD
    }

    /// Add `n` to the Limit meter, saturating at the break threshold. Returns
    /// the new Limit value. Callers decide what to do when
    /// `is_at_limit_break()` becomes true (resolve the Virtue-Flaw scene,
    /// then reset to 0).
    pub fn add_limit(&mut self, n: u8) -> u8 {
        self.limit = self
            .limit
            .saturating_add(n)
            .min(Self::LIMIT_BREAK_THRESHOLD);
        self.limit
    }

    /// Remaining Virtue channels in the current story for `virtue`, given
    /// the character's current Virtue rating. Returns 0 if the player has
    /// already used all dots' worth this story.
    pub fn channels_remaining(&self, virtue: VirtueKind, virtue_dots: u8) -> u8 {
        let used = self.virtue_channels_used.get(&virtue).copied().unwrap_or(0);
        virtue_dots.saturating_sub(used)
    }

    /// Spend one channel of `virtue` this story. Returns `true` if the
    /// channel was available (and consumed), `false` if the character has
    /// already exhausted this Virtue's channels.
    pub fn use_channel(&mut self, virtue: VirtueKind, virtue_dots: u8) -> bool {
        if self.channels_remaining(virtue, virtue_dots) == 0 {
            return false;
        }
        *self.virtue_channels_used.entry(virtue).or_insert(0) += 1;
        true
    }

    /// Add `n` motes to the spent counter for `pool`. Saturating; no max
    /// check (callers should consult `essence::essence_*_available` first if
    /// they care). The pools section in the UI already lets the user edit the
    /// raw counter past the max, so this matches existing behavior.
    pub fn spend_motes(&mut self, pool: MotePool, n: u16) {
        match pool {
            MotePool::Personal => {
                self.personal_motes_spent = self.personal_motes_spent.saturating_add(n);
            }
            MotePool::Peripheral => {
                self.peripheral_motes_spent = self.peripheral_motes_spent.saturating_add(n);
            }
        }
    }
}
