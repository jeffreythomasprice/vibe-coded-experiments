use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chess_shared::{
    BoardState, CreateGameRequest, Game, GameStatus, GameSummary, GameVariant, ListGamesResponse,
    Move, ParticipantInput, ParticipantInputKind, Piece, PieceColor, PieceType, Player, PlayerKind,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::extractors::ValidatedJson;
use crate::AppState;

#[derive(Deserialize)]
pub struct ListGamesParams {
    pub page_size: Option<u32>,
    pub page_token: Option<String>,
}

#[derive(Deserialize, serde::Serialize)]
struct Cursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn forbidden(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": msg})),
    )
}

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": "bad_request", "message": msg})),
    )
}

fn internal_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal_error"})),
    )
}

fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "not_found"})),
    )
}

pub async fn list_games(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<ListGamesParams>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let page_size = params.page_size.unwrap_or(20).min(100) as i64;
    let fetch_limit = page_size + 1;

    let cursor = params
        .page_token
        .as_deref()
        .map(|token| {
            let bytes = URL_SAFE_NO_PAD
                .decode(token)
                .map_err(|_| forbidden("invalid_page_token"))?;
            serde_json::from_slice::<Cursor>(&bytes)
                .map_err(|_| forbidden("invalid_page_token"))
        })
        .transpose()?;

    type GameRow = (
        Uuid,            // id
        String,          // variant
        String,          // status
        Option<String>,  // player_white_name
        Option<String>,  // player_white_kind
        Option<String>,  // player_black_name
        Option<String>,  // player_black_kind
        Option<String>,  // active_color
        DateTime<Utc>,   // created_at
        DateTime<Utc>,   // updated_at
    );

    let rows: Vec<GameRow> = if auth_user.is_admin {
        if let Some(ref cursor) = cursor {
            sqlx::query_as(
                "SELECT id, variant, status, \
                 state->'player_white'->>'name', state->'player_white'->>'kind', \
                 state->'player_black'->>'name', state->'player_black'->>'kind', \
                 state->>'active_color', \
                 created_at, updated_at \
                 FROM games \
                 WHERE (created_at, id) > ($1, $2) \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $3",
            )
            .bind(cursor.created_at)
            .bind(cursor.id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        } else {
            sqlx::query_as(
                "SELECT id, variant, status, \
                 state->'player_white'->>'name', state->'player_white'->>'kind', \
                 state->'player_black'->>'name', state->'player_black'->>'kind', \
                 state->>'active_color', \
                 created_at, updated_at \
                 FROM games \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        }
    } else {
        let username = &auth_user.username;
        if let Some(ref cursor) = cursor {
            sqlx::query_as(
                "SELECT id, variant, status, \
                 state->'player_white'->>'name', state->'player_white'->>'kind', \
                 state->'player_black'->>'name', state->'player_black'->>'kind', \
                 state->>'active_color', \
                 created_at, updated_at \
                 FROM games \
                 WHERE (state->'player_white'->>'name' = $1 OR state->'player_black'->>'name' = $1) \
                 AND (created_at, id) > ($2, $3) \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $4",
            )
            .bind(username)
            .bind(cursor.created_at)
            .bind(cursor.id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        } else {
            sqlx::query_as(
                "SELECT id, variant, status, \
                 state->'player_white'->>'name', state->'player_white'->>'kind', \
                 state->'player_black'->>'name', state->'player_black'->>'kind', \
                 state->>'active_color', \
                 created_at, updated_at \
                 FROM games \
                 WHERE state->'player_white'->>'name' = $1 OR state->'player_black'->>'name' = $1 \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $2",
            )
            .bind(username)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        }
    };

    let has_next = rows.len() as i64 > page_size;
    let games: Vec<GameSummary> = rows
        .into_iter()
        .take(page_size as usize)
        .map(
            |(id, variant, status, pw_name, pw_kind, pb_name, pb_kind, active_color, created_at, updated_at)| {
                GameSummary {
                    id,
                    variant,
                    status,
                    player_white_name: pw_name.unwrap_or_default(),
                    player_white_kind: pw_kind.unwrap_or_default(),
                    player_black_name: pb_name.unwrap_or_default(),
                    player_black_kind: pb_kind.unwrap_or_default(),
                    active_color,
                    created_at,
                    updated_at,
                }
            },
        )
        .collect();

    let next_page_token = if has_next {
        games.last().map(|g| {
            let cursor = Cursor {
                created_at: g.created_at,
                id: g.id,
            };
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap())
        })
    } else {
        None
    };

    Ok((
        StatusCode::OK,
        Json(ListGamesResponse {
            games,
            next_page_token,
        }),
    ))
}

pub async fn create_game(
    auth_user: AuthUser,
    State(state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<CreateGameRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Validate: human participants must have username, AI must not
    for (label, p) in [("white", &payload.player_white), ("black", &payload.player_black)] {
        match p.kind {
            ParticipantInputKind::Human => {
                if p.username.is_none() {
                    return Err(bad_request(&format!(
                        "{label} participant is human but missing username"
                    )));
                }
            }
            ParticipantInputKind::Ai => {
                if p.username.is_some() {
                    return Err(bad_request(&format!(
                        "{label} participant is AI but has username"
                    )));
                }
            }
        }
    }

    // Non-admins must be a participant
    if !auth_user.is_admin {
        let is_white = payload
            .player_white
            .username
            .as_ref()
            .map(|u| u.as_str() == auth_user.username)
            .unwrap_or(false);
        let is_black = payload
            .player_black
            .username
            .as_ref()
            .map(|u| u.as_str() == auth_user.username)
            .unwrap_or(false);
        if !is_white && !is_black {
            return Err(forbidden("must_be_participant"));
        }
    }

    // Validate AI engine availability
    let variant = match payload.variant {
        chess_shared::CreateGameRequestVariant::Standard => GameVariant::Standard,
        chess_shared::CreateGameRequestVariant::ForcedCaptureLoseAll => {
            GameVariant::ForcedCaptureLoseAll
        }
        chess_shared::CreateGameRequestVariant::ForcedCaptureCheckmate => {
            GameVariant::ForcedCaptureCheckmate
        }
    };

    for (label, p) in [("white", &payload.player_white), ("black", &payload.player_black)] {
        if p.kind == ParticipantInputKind::Ai {
            let engine_name = p.ai_engine.as_deref().unwrap_or("random");
            let engine = state.ai_manager.registry.get(engine_name).ok_or_else(|| {
                bad_request(&format!("unknown AI engine '{engine_name}' for {label}"))
            })?;
            if !engine.supports_variant(&variant) {
                return Err(bad_request(&format!(
                    "AI engine '{engine_name}' does not support variant '{}'",
                    variant
                )));
            }
        }
    }

    // Resolve players
    let player_white = resolve_player(&payload.player_white, &state)
        .await?;
    let player_black = resolve_player(&payload.player_black, &state)
        .await?;

    let creator_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| internal_error())?;

    let game = Game {
        id: Uuid::new_v4(),
        variant,
        status: GameStatus::Active,
        player_white: player_white.clone(),
        player_black: player_black.clone(),
        board: default_board(),
        active_color: Some(PieceColor::White),
        move_history: Vec::new(),
        ai_thinking: Some(false),
        resigned_by: None,
        created_by: Some(creator_id),
    };

    let game_json =
        serde_json::to_value(&game).map_err(|_| internal_error())?;

    let row = sqlx::query_as::<_, (Uuid, DateTime<Utc>, DateTime<Utc>)>(
        "INSERT INTO games (id, variant, status, state, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now(), now()) \
         RETURNING id, created_at, updated_at",
    )
    .bind(game.id)
    .bind(game.variant.to_string())
    .bind(game.status.to_string())
    .bind(&game_json)
    .bind(creator_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| internal_error())?;

    // If white is AI, schedule the first move
    let white_is_ai = player_white.kind == PlayerKind::Ai;
    let game_id = game.id;

    let response = (
        StatusCode::CREATED,
        Json(GameSummary {
            id: row.0,
            variant: game.variant.to_string(),
            status: game.status.to_string(),
            player_white_name: player_white.name,
            player_white_kind: player_white.kind.to_string(),
            player_black_name: player_black.name,
            player_black_kind: player_black.kind.to_string(),
            active_color: Some("white".to_string()),
            created_at: row.1,
            updated_at: row.2,
        }),
    );

    if white_is_ai {
        state.ai_manager.schedule_move(game_id);
    }

    Ok(response)
}

pub async fn get_game(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let row: Option<(serde_json::Value, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT state, \
         state->'player_white'->>'name', \
         state->'player_black'->>'name' \
         FROM games WHERE id = $1",
    )
    .bind(game_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| internal_error())?;

    let (game_state, pw_name, pb_name) = row.ok_or_else(not_found)?;

    if !auth_user.is_admin {
        let is_participant = [pw_name.as_deref(), pb_name.as_deref()]
            .iter()
            .any(|name| *name == Some(&auth_user.username));
        if !is_participant {
            return Err(forbidden("not_authorized"));
        }
    }

    Ok((StatusCode::OK, Json(game_state)))
}

pub async fn delete_game(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if !auth_user.is_admin {
        return Err(forbidden("admin_required"));
    }

    let result = sqlx::query("DELETE FROM games WHERE id = $1")
        .bind(game_id)
        .execute(&state.db)
        .await
        .map_err(|_| internal_error())?;

    if result.rows_affected() == 0 {
        return Err(not_found());
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn resolve_player(
    input: &ParticipantInput,
    state: &AppState,
) -> Result<Player, (StatusCode, Json<serde_json::Value>)> {
    let kind = &input.kind;
    let username = &input.username;
    match kind {
        ParticipantInputKind::Human => {
            let uname = username.as_ref().unwrap();
            let row = sqlx::query_as::<_, (Uuid, String)>(
                "SELECT id, username FROM users WHERE username = $1",
            )
            .bind(uname.as_str())
            .fetch_optional(&state.db)
            .await
            .map_err(|_| internal_error())?;

            let (id, name) = row.ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "user_not_found",
                        "message": format!("user '{}' not found", uname.as_str())
                    })),
                )
            })?;

            Ok(Player {
                id,
                name,
                kind: PlayerKind::Human,
                ai_engine: None,
                ai_params: Default::default(),
            })
        }
        ParticipantInputKind::Ai => Ok(Player {
            id: Uuid::new_v4(),
            name: "AI".to_string(),
            kind: PlayerKind::Ai,
            ai_engine: Some(
                input
                    .ai_engine
                    .as_deref()
                    .unwrap_or("random")
                    .to_string(),
            ),
            ai_params: input.ai_params.clone(),
        }),
    }
}

fn default_board() -> BoardState {
    use PieceColor::*;
    use PieceType::*;

    let mut pieces = std::collections::HashMap::new();

    let back_rank = [Rook, Knight, Bishop, Queen, King, Bishop, Knight, Rook];
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];

    for (i, &piece_type) in back_rank.iter().enumerate() {
        let file = files[i];
        pieces.insert(
            format!("{file}1"),
            Piece {
                piece_type,
                color: White,
            },
        );
        pieces.insert(
            format!("{file}8"),
            Piece {
                piece_type,
                color: Black,
            },
        );
        pieces.insert(
            format!("{file}2"),
            Piece {
                piece_type: Pawn,
                color: White,
            },
        );
        pieces.insert(
            format!("{file}7"),
            Piece {
                piece_type: Pawn,
                color: Black,
            },
        );
    }

    BoardState { pieces }
}

pub async fn make_move(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(game_id): Path<Uuid>,
    ValidatedJson(payload): ValidatedJson<Move>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Load game
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT state FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| internal_error())?;

    let (state_json,) = row.ok_or_else(not_found)?;
    let mut game: Game =
        serde_json::from_value(state_json).map_err(|_| internal_error())?;

    // Check game is active
    if !matches!(game.status, GameStatus::Active | GameStatus::Check) {
        return Err(bad_request("game is not active"));
    }

    // Check if AI is thinking
    if game.ai_thinking == Some(true) {
        return Err(bad_request("AI is currently thinking"));
    }

    // Determine active player
    let active_color = game
        .active_color
        .as_ref()
        .ok_or_else(|| bad_request("no active color"))?;

    let active_player = match active_color {
        PieceColor::White => &game.player_white,
        PieceColor::Black => &game.player_black,
    };

    // Must be the active player
    if active_player.kind != PlayerKind::Human {
        return Err(bad_request("it is the AI's turn"));
    }
    if active_player.name != auth_user.username {
        return Err(forbidden("not_your_turn"));
    }

    // Validate move is legal
    let legal = chess_engine::legal_moves(&game.board, active_color, &game.variant, &game.move_history);
    if !legal.iter().any(|m| m.from == payload.from && m.to == payload.to && m.promotion == payload.promotion) {
        return Err(bad_request("illegal move"));
    }

    // Apply move
    chess_engine::apply_move(&mut game.board, &payload);
    game.move_history.push(payload);

    let next_color = match active_color {
        PieceColor::White => PieceColor::Black,
        PieceColor::Black => PieceColor::White,
    };

    let new_status = chess_engine::game_status(&game.board, &next_color, &game.variant, &game.move_history);
    game.status = new_status;
    game.active_color = Some(next_color);

    // Save
    let game_json = serde_json::to_value(&game).map_err(|_| internal_error())?;
    let status_str = serde_json::to_value(new_status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "active".to_string());

    sqlx::query("UPDATE games SET state = $1, status = $2, updated_at = now() WHERE id = $3")
        .bind(&game_json)
        .bind(&status_str)
        .bind(game_id)
        .execute(&state.db)
        .await
        .map_err(|_| internal_error())?;

    // If next player is AI and game is still active, schedule AI move
    if matches!(new_status, GameStatus::Active | GameStatus::Check) {
        let next_player = match next_color {
            PieceColor::White => &game.player_white,
            PieceColor::Black => &game.player_black,
        };
        if next_player.kind == PlayerKind::Ai {
            state.ai_manager.schedule_move(game_id);
        }
    }

    Ok((StatusCode::OK, Json(game_json)))
}

pub async fn legal_moves(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Load game
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT state FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| internal_error())?;

    let (state_json,) = row.ok_or_else(not_found)?;
    let game: Game =
        serde_json::from_value(state_json).map_err(|_| internal_error())?;

    // Auth: must be admin or participant
    if !auth_user.is_admin {
        let is_participant = game.player_white.name == auth_user.username
            || game.player_black.name == auth_user.username;
        if !is_participant {
            return Err(forbidden("not_participant"));
        }
    }

    let active_color = game
        .active_color
        .as_ref()
        .ok_or_else(|| bad_request("no active color"))?;

    let moves = chess_engine::legal_moves(&game.board, active_color, &game.variant, &game.move_history);

    Ok((StatusCode::OK, Json(moves)))
}

pub async fn abandon_game(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let row: Option<(serde_json::Value, Option<Uuid>)> =
        sqlx::query_as("SELECT state, created_by FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| internal_error())?;

    let (state_json, created_by) = row.ok_or_else(not_found)?;
    let mut game: Game =
        serde_json::from_value(state_json).map_err(|_| internal_error())?;

    if !matches!(game.status, GameStatus::Active | GameStatus::Check) {
        return Err(bad_request("game is not active"));
    }

    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| internal_error())?;
    let is_ai_vs_ai =
        game.player_white.kind == PlayerKind::Ai && game.player_black.kind == PlayerKind::Ai;
    let is_human_player = game.player_white.name == auth_user.username
        && game.player_white.kind == PlayerKind::Human
        || game.player_black.name == auth_user.username
            && game.player_black.kind == PlayerKind::Human;

    if !auth_user.is_admin {
        let allowed = if is_ai_vs_ai {
            created_by == Some(user_id)
        } else {
            is_human_player
        };
        if !allowed {
            return Err(forbidden("not_authorized"));
        }
    }

    // Cancel AI tasks
    state.ai_manager.cancel_game(game_id);

    game.status = GameStatus::Abandoned;
    game.ai_thinking = Some(false);

    let game_json = serde_json::to_value(&game).map_err(|_| internal_error())?;

    sqlx::query(
        "UPDATE games SET state = $1, status = 'abandoned', ai_thinking = false, updated_at = now() WHERE id = $2",
    )
    .bind(&game_json)
    .bind(game_id)
    .execute(&state.db)
    .await
    .map_err(|_| internal_error())?;

    let _ = state
        .ai_manager
        .event_tx()
        .send(crate::ai::manager::GameEvent::GameAbandoned { game_id });

    Ok((StatusCode::OK, Json(game_json)))
}

pub async fn surrender_game(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(game_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT state FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| internal_error())?;

    let (state_json,) = row.ok_or_else(not_found)?;
    let mut game: Game =
        serde_json::from_value(state_json).map_err(|_| internal_error())?;

    if !matches!(game.status, GameStatus::Active | GameStatus::Check) {
        return Err(bad_request("game is not active"));
    }

    // Determine which color the user is
    let user_color = if game.player_white.name == auth_user.username
        && game.player_white.kind == PlayerKind::Human
    {
        "white"
    } else if game.player_black.name == auth_user.username
        && game.player_black.kind == PlayerKind::Human
    {
        "black"
    } else {
        return Err(forbidden("not_participant"));
    };

    // Cancel AI tasks
    state.ai_manager.cancel_game(game_id);

    game.status = GameStatus::Resigned;
    game.ai_thinking = Some(false);
    game.resigned_by = Some(match user_color {
        "white" => PieceColor::White,
        _ => PieceColor::Black,
    });

    let game_json = serde_json::to_value(&game).map_err(|_| internal_error())?;

    sqlx::query(
        "UPDATE games SET state = $1, status = 'resigned', ai_thinking = false, updated_at = now() WHERE id = $2",
    )
    .bind(&game_json)
    .bind(game_id)
    .execute(&state.db)
    .await
    .map_err(|_| internal_error())?;

    let _ = state
        .ai_manager
        .event_tx()
        .send(crate::ai::manager::GameEvent::GameResigned {
            game_id,
            status: "resigned".to_string(),
            resigned_by: user_color.to_string(),
        });

    Ok((StatusCode::OK, Json(game_json)))
}
