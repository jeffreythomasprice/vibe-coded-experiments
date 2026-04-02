use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use uuid::Uuid;

use chess_shared::*;

use super::registry::AiRegistry;

/// Delay between consecutive AI moves in AI-vs-AI games.
const AI_MOVE_DELAY_MS: u64 = 500;

/// Events broadcast to WebSocket subscribers.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameEvent {
    AiThinkingStarted {
        game_id: Uuid,
    },
    MoveMade {
        game_id: Uuid,
        #[serde(rename = "move")]
        chess_move: Move,
        status: String,
        board: BoardState,
        active_color: Option<PieceColor>,
    },
    AiThinkingCompleted {
        game_id: Uuid,
    },
    AiProgress {
        game_id: Uuid,
        message: String,
    },
}

/// Manages background AI move generation tasks.
pub struct AiManager {
    pub registry: Arc<AiRegistry>,
    db: PgPool,
    active_tasks: Arc<DashMap<Uuid, JoinHandle<()>>>,
    event_tx: broadcast::Sender<GameEvent>,
}

impl AiManager {
    pub fn new(registry: Arc<AiRegistry>, db: PgPool) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            registry,
            db,
            active_tasks: Arc::new(DashMap::new()),
            event_tx,
        }
    }

    /// Get a broadcast receiver for game events.
    pub fn subscribe(&self) -> broadcast::Receiver<GameEvent> {
        self.event_tx.subscribe()
    }

    /// Schedule AI move generation for a game.
    pub fn schedule_move(self: &Arc<Self>, game_id: Uuid) {
        let manager = Arc::clone(self);
        let handle = tokio::spawn(async move {
            if let Err(e) = manager.execute_move(game_id).await {
                tracing::error!(game_id = %game_id, error = %e, "AI move generation failed");
            }
            manager.active_tasks.remove(&game_id);
        });
        self.active_tasks.insert(game_id, handle);
    }

    async fn execute_move(self: &Arc<Self>, game_id: Uuid) -> Result<(), String> {
        // Load game state from DB
        let row: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT state FROM games WHERE id = $1")
                .bind(game_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| format!("DB error: {e}"))?;

        let (state_json,) = row.ok_or("Game not found")?;
        let game: Game =
            serde_json::from_value(state_json).map_err(|e| format!("Parse error: {e}"))?;

        let active_color = *game
            .active_color
            .as_ref()
            .ok_or("No active color")?;

        // Determine which player is active and get their engine
        let active_player = match active_color {
            PieceColor::White => &game.player_white,
            PieceColor::Black => &game.player_black,
        };

        if active_player.kind != PlayerKind::Ai {
            return Ok(()); // Not an AI player, nothing to do
        }

        let engine_name = active_player
            .ai_engine
            .as_deref()
            .unwrap_or("random");

        let engine = self
            .registry
            .get(engine_name)
            .ok_or_else(|| format!("Unknown AI engine: {engine_name}"))?;

        // Set ai_thinking = true
        self.set_ai_thinking(game_id, &game, true).await?;
        let _ = self.event_tx.send(GameEvent::AiThinkingStarted { game_id });

        // Generate move in a blocking thread
        let board = game.board.clone();
        let variant = game.variant;
        let history = game.move_history.clone();
        let color = active_color;
        let ai_params = active_player.ai_params.clone();

        let chess_move = tokio::task::spawn_blocking(move || {
            engine.generate_move(&board, &color, &variant, &history, &ai_params)
        })
        .await
        .map_err(|e| format!("Task join error: {e}"))?;

        let Some(chess_move) = chess_move else {
            // No legal moves — game should already be in a terminal state
            let _ = self
                .event_tx
                .send(GameEvent::AiThinkingCompleted { game_id });
            return Ok(());
        };

        // Apply the move and update game state
        let mut updated_game = game;
        chess_engine::apply_move(&mut updated_game.board, &chess_move);
        updated_game.move_history.push(chess_move.clone());

        let next_color = opposite_color(&active_color);
        let new_status = chess_engine::game_status(
            &updated_game.board,
            &next_color,
            &updated_game.variant,
            &updated_game.move_history,
        );

        updated_game.status = new_status;
        updated_game.active_color = Some(next_color);
        updated_game.ai_thinking = Some(false);

        // Save to DB
        let state_json =
            serde_json::to_value(&updated_game).map_err(|e| format!("Serialize error: {e}"))?;

        let status_str = serde_json::to_value(new_status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "active".to_string());

        sqlx::query(
            "UPDATE games SET state = $1, status = $2, ai_thinking = false, updated_at = now() WHERE id = $3",
        )
        .bind(&state_json)
        .bind(&status_str)
        .bind(game_id)
        .execute(&self.db)
        .await
        .map_err(|e| format!("DB update error: {e}"))?;

        let _ = self.event_tx.send(GameEvent::MoveMade {
            game_id,
            chess_move,
            status: status_str.clone(),
            board: updated_game.board.clone(),
            active_color: Some(next_color),
        });
        let _ = self
            .event_tx
            .send(GameEvent::AiThinkingCompleted { game_id });

        // If the game is still active and the next player is also AI, schedule another move
        if is_game_active(&new_status) {
            let next_player = match next_color {
                PieceColor::White => &updated_game.player_white,
                PieceColor::Black => &updated_game.player_black,
            };
            if next_player.kind == PlayerKind::Ai {
                // Small delay for AI-vs-AI to avoid busy-spinning
                tokio::time::sleep(std::time::Duration::from_millis(AI_MOVE_DELAY_MS)).await;
                self.schedule_move(game_id);
            }
        }

        Ok(())
    }

    async fn set_ai_thinking(
        &self,
        game_id: Uuid,
        game: &Game,
        thinking: bool,
    ) -> Result<(), String> {
        let mut updated = game.clone();
        updated.ai_thinking = Some(thinking);
        let state_json =
            serde_json::to_value(&updated).map_err(|e| format!("Serialize error: {e}"))?;

        sqlx::query("UPDATE games SET state = $1, ai_thinking = $2, updated_at = now() WHERE id = $3")
            .bind(&state_json)
            .bind(thinking)
            .bind(game_id)
            .execute(&self.db)
            .await
            .map_err(|e| format!("DB update error: {e}"))?;

        Ok(())
    }

    /// Re-schedule AI moves for games that were thinking when the server stopped.
    pub async fn recover_thinking_games(self: &Arc<Self>) {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM games WHERE ai_thinking = true AND status IN ('active', 'check')",
        )
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        for (game_id,) in rows {
            tracing::info!(game_id = %game_id, "Recovering AI thinking task");
            self.schedule_move(game_id);
        }
    }
}

fn is_game_active(status: &GameStatus) -> bool {
    matches!(status, GameStatus::Active | GameStatus::Check)
}

fn opposite_color(color: &PieceColor) -> PieceColor {
    match color {
        PieceColor::White => PieceColor::Black,
        PieceColor::Black => PieceColor::White,
    }
}
