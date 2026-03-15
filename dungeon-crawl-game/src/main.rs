use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use tracing_subscriber::EnvFilter;

mod dungeon;
mod generator;
mod loader;
mod placement;
mod players;
mod rendering;
mod room_queue;
mod schema_types;
mod sidebar;
mod turn;
mod types;
mod ui;

use rendering::{CameraState, DragState};
use room_queue::RoomQueue;
use schema_types::Player;
use types::*;

fn main() {
    let pkg = env!("CARGO_PKG_NAME").replace('-', "_");
    let default_filter = format!("{pkg}=trace,warn");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&default_filter)),
        )
        .init();

    let config = loader::load_config("config.yaml").expect("Failed to load config");
    let themes = loader::load_theme("assets/theme.yaml").expect("Failed to load theme");

    let queue = RoomQueue::new(config.clone(), themes.clone(), 3);

    let pending_players = Arc::new(Mutex::new(None));
    let pending_clone = pending_players.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::from_os_rng();
        rt.block_on(async {
            let mut generated = Vec::new();
            for _ in 0..4 {
                let existing: Vec<String> = generated.iter().map(|p: &Player| p.name.clone()).collect();
                match generator::generate_player(&mut rng, &themes, &config, &existing).await {
                    Ok(p) => {
                        tracing::info!("Generated player: {}", p.name);
                        generated.push(p);
                    }
                    Err(e) => tracing::error!("Player generation failed: {e}"),
                }
            }
            *pending_clone.lock().unwrap() = Some(generated);
        });
    });

    App::new()
        .add_plugins(DefaultPlugins.build().disable::<bevy::log::LogPlugin>())
        .init_state::<GameState>()
        .insert_resource(queue)
        .insert_resource(dungeon::Dungeon::default())
        .insert_resource(CameraState { zoom: 1.0, offset: Vec2::ZERO })
        .insert_resource(DragState::default())
        .insert_resource(HoveredRoom::default())
        .insert_resource(HoveredPlayer::default())
        .insert_resource(TurnNumber::default())
        .insert_resource(SelectedPlayer::default())
        .insert_resource(PendingPlayers(pending_players))
        .add_systems(Startup, ui::setup_ui)
        .add_systems(
            Update,
            (
                ui::update_room_count,
                rendering::camera_keyboard_pan,
                rendering::camera_zoom,
                rendering::camera_drag_pan,
                rendering::apply_camera,
                ui::handle_reset_view,
                sidebar::detect_hovered_room,
                sidebar::update_sidebar,
                players::check_pending_players,
                players::update_player_hud,
                players::update_player_dots,
                players::update_selected_player_highlight,
                players::update_selected_hud_highlight,
                sidebar::detect_player_hover,
                sidebar::update_player_sidebar,
                ui::update_turn_display,
                ui::update_move_display,
                ui::update_end_turn_visibility,
            ),
        )
        .add_systems(
            Update,
            (turn::auto_place_first_room, turn::auto_start_first_turn).run_if(in_state(GameState::WaitingForSetup)),
        )
        .add_systems(
            Update,
            (turn::handle_player_selection, turn::handle_destination_click, turn::check_all_destinations_set, turn::update_reachable_highlights, turn::handle_end_turn)
                .run_if(in_state(GameState::SelectingDestinations)),
        )
        .add_systems(
            Update,
            (turn::advance_movement, turn::handle_end_turn).run_if(in_state(GameState::Moving)),
        )
        .add_systems(
            Update,
            (placement::update_ghost_preview, placement::handle_rotation, placement::handle_ghost_click)
                .run_if(in_state(GameState::Placing)),
        )
        .add_systems(OnEnter(GameState::StartingTurn), turn::enter_starting_turn)
        .add_systems(OnEnter(GameState::SelectingDestinations), (turn::enter_selecting, turn::enter_reselecting).chain())
        .add_systems(OnExit(GameState::SelectingDestinations), turn::exit_selecting)
        .add_systems(OnEnter(GameState::Moving), turn::enter_moving)
        .add_systems(OnEnter(GameState::RevealingRoom), placement::enter_revealing)
        .add_systems(OnEnter(GameState::Placing), placement::enter_placing)
        .add_systems(OnExit(GameState::Placing), placement::exit_placing)
        .run();
}
