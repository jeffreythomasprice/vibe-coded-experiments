use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

use crate::effects::EffectSourceKind;
use crate::schema_types::{Effect, Player, StatName};

#[derive(Resource, Default)]
pub struct HoveredRoom(pub Option<IVec2>);

#[derive(Resource, Default)]
pub struct GhostRotation(pub u8);

#[derive(Resource, Default)]
pub struct HoveredCandidate(pub Option<IVec2>);

#[derive(Component)]
pub struct SidebarPanel;

#[derive(Component)]
pub struct SidebarTitle;

#[derive(Component)]
pub struct SidebarDescription;

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    WaitingForSetup,
    StartingTurn,
    SelectingDestinations,
    Moving,
    RevealingRoom,
    Placing,
    AllocatingDamage,
    ShowingEffectLog,
}

#[derive(Resource)]
pub struct PendingRoom(pub crate::schema_types::Room);

#[derive(Component)]
pub struct RoomCountText;

#[derive(Component)]
pub struct ResetViewButton;

#[derive(Component)]
pub struct EndTurnButton;

#[derive(Resource, Default)]
pub struct TurnNumber(pub u32);

#[derive(Resource)]
pub struct MoveTimer(pub Timer);

#[derive(Resource)]
pub struct RevealingPlayer(pub usize);

#[derive(Resource, Default)]
pub struct SelectedPlayer(pub Option<usize>);

#[derive(Component)]
pub struct SelectedPlayerHighlight;

#[derive(Resource)]
pub struct RevealCell(pub IVec2);

pub const PLAYER_COLORS: [Color; 4] = [
    Color::srgb(0.9, 0.2, 0.2),
    Color::srgb(0.2, 0.4, 0.9),
    Color::srgb(0.9, 0.6, 0.1),
    Color::srgb(0.7, 0.3, 0.9),
];

#[derive(Debug, Clone)]
pub struct ActiveEffect {
    pub effect: Effect,
    pub source_room: IVec2,
    pub source_kind: crate::effects::EffectSourceKind,
    pub source_name: String,
    pub source_description: String,
    pub remaining_turns: Option<u32>,
    pub delay_turns: Option<u32>,
}

pub struct PlayerInfo {
    pub player: Player,
    pub color: Color,
    pub location: IVec2,
    pub remaining_move: i64,
    pub destination: Option<IVec2>,
    pub path: VecDeque<IVec2>,
    pub active_effects: Vec<ActiveEffect>,
    pub inventory: Vec<crate::schema_types::Item>,
    pub dead: bool,
}

#[derive(Resource)]
pub struct Players(pub Vec<PlayerInfo>);

#[derive(Resource)]
pub struct PendingPlayers(pub Arc<Mutex<Option<Vec<Player>>>>);

#[derive(Component)]
pub struct PlayerHudSlot(pub usize);

#[derive(Component)]
pub struct PlayerHudName(pub usize);

#[derive(Component)]
pub struct PlayerHudStats(pub usize);

#[derive(Component)]
pub struct PlayerHudMove(pub usize);

#[derive(Component)]
pub struct PlayerHudColor(pub usize);

#[derive(Component)]
pub struct SidebarPlayers;

#[derive(Resource, Default)]
pub struct HoveredPlayer(pub Option<usize>);

#[derive(Component)]
pub struct PlayerSidebarPanel;

#[derive(Component)]
pub struct PlayerSidebarName;

#[derive(Component)]
pub struct PlayerSidebarDescription;

#[derive(Component)]
pub struct PlaceholderRoom;

#[derive(Resource, Default)]
pub struct EffectLogBuffer(pub Vec<crate::effects::EffectLog>);

#[derive(Resource)]
pub struct ResumeState(pub GameState);

#[derive(Component)]
pub struct EffectLogPopup;

#[derive(Component)]
pub struct EffectLogDismissButton;

#[derive(Component)]
pub struct TurnNumberText;

#[derive(Component)]
pub struct StatusText;

#[derive(Resource)]
pub struct Reselecting;

#[derive(Component)]
pub struct ReachableMarker;

#[derive(Component)]
pub struct PathPreviewMarker;

#[derive(Component)]
pub struct DestinationArrow;

#[derive(Component)]
pub struct SidebarEffects;

#[derive(Component)]
pub struct PlayerSidebarStats;

#[derive(Component)]
pub struct PlayerSidebarEffects;

#[derive(Component)]
pub struct PlayerSidebarInventory;

#[derive(Component)]
pub struct PlayerHudDead(pub usize);

#[derive(Resource)]
pub struct TickEffectsProgress(pub usize);

#[derive(Debug, Clone)]
pub struct PendingDamageAllocation {
    pub player_idx: usize,
    pub total: u32,
    pub stat_a: StatName,
    pub stat_b: StatName,
    pub sign: i64,
    pub source_kind: EffectSourceKind,
    pub source_name: String,
    pub source_description: String,
}

#[derive(Resource, Default)]
pub struct DamageAllocationQueue(pub VecDeque<PendingDamageAllocation>);

#[derive(Resource)]
pub struct CurrentAllocation {
    pub pending: PendingDamageAllocation,
    pub stat_a_value: u32,
    pub stat_b_value: u32,
}

#[derive(Component)]
pub struct DamageAllocationPopup;

#[derive(Component)]
pub struct DamageStatAText;

#[derive(Component)]
pub struct DamageStatBText;

#[derive(Component)]
pub struct DamageTotalText;

#[derive(Component)]
pub struct DamageStatAUp;

#[derive(Component)]
pub struct DamageStatADown;

#[derive(Component)]
pub struct DamageStatBUp;

#[derive(Component)]
pub struct DamageStatBDown;

#[derive(Component)]
pub struct DamageConfirmButton;
