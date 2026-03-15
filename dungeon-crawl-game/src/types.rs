use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

use crate::schema_types::{Effect, Player};

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
pub struct SidebarEffects;

#[derive(Component)]
pub struct PlayerSidebarEffects;

#[derive(Component)]
pub struct PlayerHudDead(pub usize);
