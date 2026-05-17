use serde::{Deserialize, Serialize};

use super::traits::{AbilityKind, VirtueKind};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Caste {
    Dawn,
    Zenith,
    Twilight,
    Night,
    Eclipse,
}

impl Caste {
    pub fn caste_abilities(self) -> &'static [AbilityKind] {
        match self {
            Caste::Dawn => &[
                AbilityKind::Archery,
                AbilityKind::MartialArts,
                AbilityKind::Melee,
                AbilityKind::Thrown,
                AbilityKind::War,
            ],
            Caste::Zenith => &[
                AbilityKind::Integrity,
                AbilityKind::Performance,
                AbilityKind::Presence,
                AbilityKind::Resistance,
                AbilityKind::Survival,
            ],
            Caste::Twilight => &[
                AbilityKind::Craft,
                AbilityKind::Investigation,
                AbilityKind::Lore,
                AbilityKind::Medicine,
                AbilityKind::Occult,
            ],
            Caste::Night => &[
                AbilityKind::Athletics,
                AbilityKind::Awareness,
                AbilityKind::Dodge,
                AbilityKind::Larceny,
                AbilityKind::Stealth,
            ],
            Caste::Eclipse => &[
                AbilityKind::Bureaucracy,
                AbilityKind::Linguistics,
                AbilityKind::Ride,
                AbilityKind::Sail,
                AbilityKind::Socialize,
            ],
        }
    }
}

/// The Solar's anima totem — the burning iconic image (great golden bull,
/// sun-mandala, lion, etc.) that surrounds her at the 16+ Peripheral display
/// level. Single free-text field; the rules summary (p.75, p.117) treats
/// "Anima Totem" and "iconic image" as the same concept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anima {
    pub totem: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub hair: String,
    pub eyes: String,
    pub skin: String,
    pub distinguishing_features: String,
    pub homeland: String,
    pub sex: String,
    pub age: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    pub concept: String,
    pub motivation: String,
    /// Free-text disposition / temperament. The rules summary
    /// (`character_creation.md` §1) subsumes "personality" into Concept,
    /// but the Voidstate fillable sheet has a separate slot, so we expose
    /// it here for sheet fidelity.
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub anima: Anima,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub chronicle: String,
}

/// The Virtue Flaw chosen at chargen, driving Limit Break (p.103-107).
/// Trigger / duration notes per variant are summarised from
/// `character_creation.md` §8; full per-flaw partial-control text is not
/// encoded here (runtime mechanic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtueFlaw {
    /// Compassion. Trigger: innocents suffering through no fault of their
    /// own. Duration: 1 scene (combat) / 1 day (otherwise).
    CompassionateMartyrdom,
    /// Compassion. Trigger: innocents suffering she cannot help. Duration:
    /// 1 scene / 1 day.
    HeartOfTears,
    /// Compassion. Trigger: innocents suffering, cannot effectively
    /// intervene. Duration: 1 scene (combat) / Compassion hours otherwise.
    RedRageOfCompassion,
    /// Conviction. Trigger: severe stress or backed into a corner.
    /// Duration: 1 full day.
    DeliberateCruelty,
    /// Conviction. Trigger: frustration with intemperate world. Duration:
    /// 1 full day.
    HeartOfFlint,
    /// Temperance. Trigger: confronted with own / others' weakness.
    /// Duration: 1 full day.
    AsceticDrive,
    /// Temperance. Trigger: hindered by the self-indulgent. Duration:
    /// 1 full day.
    ContemptOfTheVirtuous,
    /// Temperance. Trigger: a favoured pleasure/vice must be passed up to
    /// act morally. Duration: 1 full day.
    Overindulgence,
    /// Valor. Trigger: insulted, demeaned, or deliberately frustrated.
    /// Duration: 1 full scene.
    BerserkAnger,
    /// Valor. Trigger: losing odds, single combat, any chance to prove
    /// bravery. Duration: 1 full day.
    FoolhardyContempt,
    Custom { name: String, virtue: VirtueKind },
}

impl VirtueFlaw {
    pub fn flaw_virtue(&self) -> VirtueKind {
        match self {
            VirtueFlaw::CompassionateMartyrdom
            | VirtueFlaw::HeartOfTears
            | VirtueFlaw::RedRageOfCompassion => VirtueKind::Compassion,
            VirtueFlaw::DeliberateCruelty | VirtueFlaw::HeartOfFlint => VirtueKind::Conviction,
            VirtueFlaw::AsceticDrive
            | VirtueFlaw::ContemptOfTheVirtuous
            | VirtueFlaw::Overindulgence => VirtueKind::Temperance,
            VirtueFlaw::BerserkAnger | VirtueFlaw::FoolhardyContempt => VirtueKind::Valor,
            VirtueFlaw::Custom { virtue, .. } => *virtue,
        }
    }
}
