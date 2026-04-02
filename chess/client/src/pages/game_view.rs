use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Clone, Debug, Deserialize)]
struct Piece {
    piece_type: String,
    color: String,
}

#[derive(Clone, Debug, Deserialize)]
struct BoardState {
    pieces: HashMap<String, Piece>,
}

#[derive(Clone, Debug, Deserialize)]
struct Player {
    name: String,
    kind: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Game {
    variant: String,
    status: String,
    player_white: Player,
    player_black: Player,
    board: BoardState,
    active_color: Option<String>,
    #[serde(default)]
    ai_thinking: Option<bool>,
    #[serde(default)]
    move_history: Vec<ChessMove>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChessMove {
    from: String,
    to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    promotion: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum GameEvent {
    AiThinkingStarted { game_id: String },
    MoveMade {
        game_id: String,
        #[serde(rename = "move")]
        chess_move: ChessMove,
        status: String,
        board: BoardState,
        active_color: Option<String>,
    },
    AiThinkingCompleted { game_id: String },
    AiProgress { game_id: String, message: String },
}

const SQUARE_SIZE: f64 = 60.0;
const BOARD_PX: f64 = SQUARE_SIZE * 8.0;
const LIGHT_SQ: &str = "#f0d9b5";
const DARK_SQ: &str = "#b58863";
const SELECTED_COLOR: &str = "rgba(20, 85, 200, 0.4)";
const LEGAL_MOVE_COLOR: &str = "rgba(20, 150, 50, 0.5)";
const LAST_MOVE_HIGHLIGHT: &str = "rgba(255, 255, 0, 0.3)";
const GHOST_ALPHA: f64 = 0.3;
const ANIM_MS_PER_SQUARE: f64 = 500.0;

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

#[derive(Clone, Debug)]
struct AnimationState {
    piece: Piece,
    from_px: (f64, f64),
    to_px: (f64, f64),
    start_time: f64,
    duration_ms: f64,
    moving_to: String,
}

fn square_to_center_px(sq: &str) -> Option<(f64, f64)> {
    let (file, rank) = parse_square(sq)?;
    Some((
        file as f64 * SQUARE_SIZE + SQUARE_SIZE / 2.0,
        (7 - rank) as f64 * SQUARE_SIZE + SQUARE_SIZE / 2.0,
    ))
}

fn animation_duration(from: &str, to: &str) -> f64 {
    let Some((f1, r1)) = parse_square(from) else { return 0.0 };
    let Some((f2, r2)) = parse_square(to) else { return 0.0 };
    let dist = ((f2 as f64 - f1 as f64).powi(2) + (r2 as f64 - r1 as f64).powi(2)).sqrt();
    dist * ANIM_MS_PER_SQUARE
}

fn draw_piece_at(ctx: &web_sys::CanvasRenderingContext2d, piece: &Piece, cx: f64, cy: f64) {
    let radius = SQUARE_SIZE * 0.35;
    let (fill, text_color) = if piece.color == "white" {
        ("#ffffff", "#333333")
    } else {
        ("#333333", "#ffffff")
    };

    ctx.begin_path();
    let _ = ctx.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
    ctx.set_fill_style_str(fill);
    ctx.fill();
    ctx.set_stroke_style_str("#000000");
    ctx.set_line_width(1.5);
    ctx.stroke();

    ctx.set_fill_style_str(text_color);
    let _ = ctx.fill_text(piece_letter(&piece.piece_type), cx, cy);
}

fn parse_square(sq: &str) -> Option<(usize, usize)> {
    let mut chars = sq.chars();
    let file = chars.next()? as usize;
    let rank = chars.next()?.to_digit(10)? as usize;
    if file < 'a' as usize || file > 'h' as usize || !(1..=8).contains(&rank) {
        return None;
    }
    Some((file - 'a' as usize, rank - 1))
}

fn square_name(file: usize, rank: usize) -> String {
    let file_char = (b'a' + file as u8) as char;
    format!("{}{}", file_char, rank + 1)
}

fn pixel_to_square(x: f64, y: f64) -> Option<String> {
    let file = (x / SQUARE_SIZE) as usize;
    let visual_row = (y / SQUARE_SIZE) as usize;
    if file > 7 || visual_row > 7 {
        return None;
    }
    let rank = 7 - visual_row;
    Some(square_name(file, rank))
}

fn piece_letter(piece_type: &str) -> &str {
    match piece_type {
        "pawn" => "P",
        "knight" => "N",
        "bishop" => "B",
        "rook" => "R",
        "queen" => "Q",
        "king" => "K",
        _ => "?",
    }
}

fn draw_board(
    ctx: &web_sys::CanvasRenderingContext2d,
    board: &BoardState,
    selected: Option<&str>,
    legal_targets: &HashSet<String>,
    last_move: Option<&ChessMove>,
    anim: Option<&AnimationState>,
    now: f64,
) {
    // Draw squares
    for rank in 0..8usize {
        for file in 0..8usize {
            let is_light = (rank + file) % 2 == 0;
            ctx.set_fill_style_str(if is_light { LIGHT_SQ } else { DARK_SQ });
            let x = file as f64 * SQUARE_SIZE;
            let y = (7 - rank) as f64 * SQUARE_SIZE;
            ctx.fill_rect(x, y, SQUARE_SIZE, SQUARE_SIZE);
        }
    }

    // Highlight last move from/to squares
    if let Some(lm) = last_move {
        for sq in [&lm.from, &lm.to] {
            if let Some((file, rank)) = parse_square(sq) {
                ctx.set_fill_style_str(LAST_MOVE_HIGHLIGHT);
                let x = file as f64 * SQUARE_SIZE;
                let y = (7 - rank) as f64 * SQUARE_SIZE;
                ctx.fill_rect(x, y, SQUARE_SIZE, SQUARE_SIZE);
            }
        }
    }

    // Highlight selected square
    if let Some(sq) = selected {
        if let Some((file, rank)) = parse_square(sq) {
            ctx.set_fill_style_str(SELECTED_COLOR);
            let x = file as f64 * SQUARE_SIZE;
            let y = (7 - rank) as f64 * SQUARE_SIZE;
            ctx.fill_rect(x, y, SQUARE_SIZE, SQUARE_SIZE);
        }
    }

    // Draw rank/file labels
    ctx.set_font("11px sans-serif");
    for file in 0..8 {
        let label = ((b'a' + file as u8) as char).to_string();
        let x = file as f64 * SQUARE_SIZE + SQUARE_SIZE - 10.0;
        let y = BOARD_PX - 3.0;
        let is_light = file % 2 == 0;
        ctx.set_fill_style_str(if is_light { DARK_SQ } else { LIGHT_SQ });
        let _ = ctx.fill_text(&label, x, y);
    }
    for rank in 0..8 {
        let label = (rank + 1).to_string();
        let x = 2.0;
        let y = (7 - rank) as f64 * SQUARE_SIZE + 12.0;
        let is_light = rank % 2 == 0;
        ctx.set_fill_style_str(if is_light { DARK_SQ } else { LIGHT_SQ });
        let _ = ctx.fill_text(&label, x, y);
    }

    // Draw ghost piece at last move's origin square
    if let Some(lm) = last_move {
        if let Some(piece) = board.pieces.get(&lm.to) {
            if let Some((cx, cy)) = square_to_center_px(&lm.from) {
                ctx.set_font("bold 20px sans-serif");
                ctx.set_text_align("center");
                ctx.set_text_baseline("middle");
                ctx.set_global_alpha(GHOST_ALPHA);
                draw_piece_at(ctx, piece, cx, cy);
                ctx.set_global_alpha(1.0);
            }
        }
    }

    // Draw pieces
    ctx.set_font("bold 20px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    let animating_sq = anim.as_ref().map(|a| a.moving_to.as_str());
    for (square, piece) in &board.pieces {
        // Skip the piece being animated — it will be drawn at interpolated position
        if animating_sq == Some(square.as_str()) {
            continue;
        }
        let Some((file, rank)) = parse_square(square) else {
            continue;
        };
        let cx = file as f64 * SQUARE_SIZE + SQUARE_SIZE / 2.0;
        let cy = (7 - rank) as f64 * SQUARE_SIZE + SQUARE_SIZE / 2.0;
        draw_piece_at(ctx, piece, cx, cy);
    }

    // Draw animating piece at interpolated position
    if let Some(a) = anim {
        let t = ((now - a.start_time) / a.duration_ms).clamp(0.0, 1.0);
        let cx = a.from_px.0 + (a.to_px.0 - a.from_px.0) * t;
        let cy = a.from_px.1 + (a.to_px.1 - a.from_px.1) * t;
        draw_piece_at(ctx, &a.piece, cx, cy);
    }

    // Draw legal move indicators (on top of pieces)
    for target in legal_targets {
        let Some((file, rank)) = parse_square(target) else {
            continue;
        };
        let cx = file as f64 * SQUARE_SIZE + SQUARE_SIZE / 2.0;
        let cy = (7 - rank) as f64 * SQUARE_SIZE + SQUARE_SIZE / 2.0;

        let is_capture = board.pieces.contains_key(target);
        ctx.set_fill_style_str(LEGAL_MOVE_COLOR);

        if is_capture {
            // Ring around piece for captures
            ctx.begin_path();
            let _ = ctx.arc(cx, cy, SQUARE_SIZE * 0.45, 0.0, std::f64::consts::TAU);
            ctx.set_stroke_style_str(LEGAL_MOVE_COLOR);
            ctx.set_line_width(4.0);
            ctx.stroke();
        } else {
            // Small dot for empty squares
            ctx.begin_path();
            let _ = ctx.arc(cx, cy, SQUARE_SIZE * 0.15, 0.0, std::f64::consts::TAU);
            ctx.fill();
        }
    }
}

fn display_variant(v: &str) -> &str {
    match v {
        "standard" => "Standard",
        "forced_capture_lose_all" => "Forced Capture (Lose All)",
        "forced_capture_checkmate" => "Forced Capture (Checkmate)",
        _ => v,
    }
}

fn display_status(s: &str) -> &str {
    match s {
        "waiting" => "Waiting",
        "active" => "In Progress",
        "check" => "Check",
        "checkmate" => "Checkmate",
        "stalemate" => "Stalemate",
        "draw" => "Draw",
        "resigned" => "Resigned",
        "abandoned" => "Abandoned",
        _ => s,
    }
}

fn get_current_username() -> String {
    crate::auth::get_token()
        .and_then(|t| {
            #[derive(Deserialize)]
            struct Claims {
                username: String,
            }
            let payload_b64url = t.split('.').nth(1)?;
            let b64: String = payload_b64url
                .chars()
                .map(|c| match c {
                    '-' => '+',
                    '_' => '/',
                    other => other,
                })
                .collect();
            let padded = match b64.len() % 4 {
                2 => format!("{b64}=="),
                3 => format!("{b64}="),
                _ => b64,
            };
            let decoded = web_sys::window()?.atob(&padded).ok()?;
            serde_json::from_str::<Claims>(&decoded)
                .ok()
                .map(|c| c.username)
        })
        .unwrap_or_default()
}

fn is_game_over(status: &str) -> bool {
    matches!(
        status,
        "checkmate" | "stalemate" | "draw" | "resigned" | "abandoned"
    )
}

#[component]
pub fn GameViewPage() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let (game, set_game) = signal(Option::<Game>::None);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(true);
    let (ai_thinking, set_ai_thinking) = signal(false);
    let selected_square = RwSignal::new(Option::<String>::None);
    let legal_moves = RwSignal::new(Vec::<ChessMove>::new());
    let pending_promotion = RwSignal::new(Option::<(String, String)>::None);
    let move_in_progress = RwSignal::new(false);
    let animation = RwSignal::new(Option::<AnimationState>::None);
    let game_id_signal = RwSignal::new(String::new());

    let current_username = get_current_username();
    let current_username_for_turn = current_username.clone();

    let is_my_turn = Memo::new(move |_| {
        let Some(g) = game.get() else {
            return false;
        };
        if is_game_over(&g.status) {
            return false;
        }
        if !matches!(g.status.as_str(), "active" | "check") {
            return false;
        }
        let Some(ref active) = g.active_color else {
            return false;
        };
        let active_player = match active.as_str() {
            "white" => &g.player_white,
            "black" => &g.player_black,
            _ => return false,
        };
        active_player.kind == "human" && active_player.name == current_username_for_turn
    });

    Effect::new(move || {
        let id = params.get().get("id");
        let Some(id) = id else {
            set_error.set(Some("Missing game ID".into()));
            set_loading.set(false);
            return;
        };

        game_id_signal.set(id.clone());
        let url = format!("/api/games/{id}");
        let ws_id = id.clone();
        leptos::task::spawn_local(async move {
            let resp = match crate::api::get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    set_error.set(Some(format!("Request failed: {e}")));
                    set_loading.set(false);
                    return;
                }
            };

            if resp.status() == 403 {
                let toast = expect_context::<crate::ToastState>();
                toast.message.set(Some("Not authorized to see that game.".into()));
                let navigate = leptos_router::hooks::use_navigate();
                navigate("/games", Default::default());
                return;
            }

            if resp.status() == 404 {
                set_error.set(Some("Game not found.".into()));
                set_loading.set(false);
                return;
            }

            if !resp.ok() {
                set_error.set(Some(format!("Server error ({})", resp.status())));
                set_loading.set(false);
                return;
            }

            match resp.json::<Game>().await {
                Ok(g) => {
                    set_ai_thinking.set(g.ai_thinking.unwrap_or(false));
                    set_game.set(Some(g));
                    set_loading.set(false);

                    connect_game_ws(
                        &ws_id,
                        set_game,
                        set_ai_thinking,
                        selected_square,
                        legal_moves,
                        game,
                        animation,
                    );
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to parse game: {e}")));
                    set_loading.set(false);
                }
            }
        });
    });

    let turn_status = move || {
        let Some(g) = game.get() else {
            return String::new();
        };
        if is_game_over(&g.status) {
            return match g.status.as_str() {
                "checkmate" => {
                    let winner = match g.active_color.as_deref() {
                        Some("white") => &g.player_black.name,
                        Some("black") => &g.player_white.name,
                        _ => "Unknown",
                    };
                    format!("Checkmate — {} wins!", winner)
                }
                "stalemate" => "Stalemate — Draw!".into(),
                "draw" => "Draw!".into(),
                "resigned" => "Game resigned.".into(),
                "abandoned" => "Game abandoned.".into(),
                _ => display_status(&g.status).to_string(),
            };
        }
        if ai_thinking.get() {
            return "AI is thinking...".into();
        }
        if is_my_turn.get() {
            return "Your turn".into();
        }
        let Some(ref active) = g.active_color else {
            return String::new();
        };
        let opponent = match active.as_str() {
            "white" => &g.player_white,
            "black" => &g.player_black,
            _ => return String::new(),
        };
        if opponent.kind == "ai" {
            "AI is thinking...".into()
        } else {
            format!("Waiting for {}...", opponent.name)
        }
    };

    view! {
        <div style="padding: 1em;">
            <a href="/games" style="margin-bottom: 1em; display: inline-block;">"← Back to Games"</a>

            {move || loading.get().then(|| view! { <p>"Loading..."</p> })}
            {move || error.get().map(|e| view! { <p style="color: red;">{e}</p> })}
            {move || game.get().map(|g| {
                let info_line = format!(
                    "{} — {}",
                    display_variant(&g.variant),
                    display_status(&g.status),
                );
                let pw = format!("{} ({})", g.player_white.name, g.player_white.kind);
                let pb = format!("{} ({})", g.player_black.name, g.player_black.kind);
                let gid = game_id_signal.get();
                view! {
                    <div style="margin-bottom: 1em;">
                        <h2>{info_line}</h2>
                        <p><strong>"White: "</strong>{pw}" vs "<strong>"Black: "</strong>{pb}</p>
                    </div>
                    <div style="margin-bottom: 0.5em;">
                        <TurnIndicator
                            text=turn_status()
                            is_my_turn=is_my_turn.get()
                            ai_thinking=ai_thinking.get()
                            game_over=is_game_over(&g.status)
                        />
                    </div>
                    <ChessBoard
                        game=game
                        selected_square=selected_square
                        legal_moves=legal_moves
                        is_my_turn=is_my_turn
                        pending_promotion=pending_promotion
                        move_in_progress=move_in_progress
                        ai_thinking=ai_thinking
                        set_ai_thinking=set_ai_thinking
                        set_game=set_game
                        game_id=gid
                        animation=animation
                    />
                    <MoveHistoryTable move_history=g.move_history.clone() />
                }
            })}
        </div>
    }
}

#[component]
fn TurnIndicator(
    text: String,
    is_my_turn: bool,
    ai_thinking: bool,
    game_over: bool,
) -> impl IntoView {
    let (bg, border, color) = if game_over {
        ("#e2e3e5", "#d6d8db", "#383d41")
    } else if ai_thinking {
        ("#fff3cd", "#ffc107", "#856404")
    } else if is_my_turn {
        ("#d4edda", "#28a745", "#155724")
    } else {
        ("#cce5ff", "#b8daff", "#004085")
    };

    let style = format!(
        "padding: 0.5em 1em; background: {bg}; border: 1px solid {border}; \
         border-radius: 4px; color: {color}; font-weight: bold;"
    );

    view! {
        <div style=style>{text}</div>
    }
}

fn connect_game_ws(
    game_id: &str,
    set_game: WriteSignal<Option<Game>>,
    set_ai_thinking: WriteSignal<bool>,
    selected_square: RwSignal<Option<String>>,
    legal_moves: RwSignal<Vec<ChessMove>>,
    game: ReadSignal<Option<Game>>,
    animation: RwSignal<Option<AnimationState>>,
) {
    let token = crate::auth::get_token().unwrap_or_default();
    let window = web_sys::window().unwrap();
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".into());
    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    let host = location.host().unwrap_or_else(|_| "localhost:8001".into());
    let (ws_final_host, ws_path) = if host.ends_with(":8000") {
        (host.replace(":8000", ":8001"), format!("/games/{game_id}/ws"))
    } else {
        (host.clone(), format!("/api/games/{game_id}/ws"))
    };
    let ws_url = format!("{ws_protocol}//{ws_final_host}{ws_path}?token={token}");

    let ws = match web_sys::WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let onmessage = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
        let Some(data) = e.data().as_string() else {
            return;
        };
        let Ok(event) = serde_json::from_str::<GameEvent>(&data) else {
            return;
        };
        match event {
            GameEvent::AiThinkingStarted { .. } => {
                set_ai_thinking.set(true);
            }
            GameEvent::MoveMade { board, status, active_color, chess_move, .. } => {
                // Capture piece at from-square before updating board for animation
                if let Some(g) = game.get() {
                    if let Some(piece) = g.board.pieces.get(&chess_move.from) {
                        if let (Some(from_px), Some(to_px)) =
                            (square_to_center_px(&chess_move.from), square_to_center_px(&chess_move.to))
                        {
                            let dur = animation_duration(&chess_move.from, &chess_move.to);
                            let now = web_sys::window()
                                .and_then(|w| w.performance())
                                .map(|p| p.now())
                                .unwrap_or(0.0);
                            animation.set(Some(AnimationState {
                                piece: piece.clone(),
                                from_px,
                                to_px,
                                start_time: now,
                                duration_ms: dur,
                                moving_to: chess_move.to.clone(),
                            }));
                        }
                    }
                }
                set_game.update(|g| {
                    if let Some(g) = g {
                        g.move_history.push(chess_move);
                        g.board = board;
                        g.status = status;
                        g.active_color = active_color;
                        g.ai_thinking = Some(false);
                    }
                });
                // Clear stale selection
                selected_square.set(None);
                legal_moves.set(vec![]);
            }
            GameEvent::AiThinkingCompleted { .. } => {
                set_ai_thinking.set(false);
            }
            GameEvent::AiProgress { .. } => {}
        }
    });

    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
}

#[component]
fn ChessBoard(
    game: ReadSignal<Option<Game>>,
    selected_square: RwSignal<Option<String>>,
    legal_moves: RwSignal<Vec<ChessMove>>,
    is_my_turn: Memo<bool>,
    pending_promotion: RwSignal<Option<(String, String)>>,
    move_in_progress: RwSignal<bool>,
    ai_thinking: ReadSignal<bool>,
    set_ai_thinking: WriteSignal<bool>,
    set_game: WriteSignal<Option<Game>>,
    game_id: String,
    animation: RwSignal<Option<AnimationState>>,
) -> impl IntoView {
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();
    let game_id_for_click = game_id.clone();
    let game_id_for_promo = game_id.clone();

    // Redraw effect
    Effect::new(move || {
        let Some(g) = game.get() else { return };
        let selected = selected_square.get();
        let moves = legal_moves.get();
        let targets: HashSet<String> = moves.iter().map(|m| m.to.clone()).collect();
        let anim = animation.get();
        let last_move = g.move_history.last();

        let Some(canvas) = canvas_ref.get() else { return };
        let canvas_el: &web_sys::HtmlCanvasElement = canvas.as_ref();
        let ctx = canvas_el
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|obj| obj.dyn_into::<web_sys::CanvasRenderingContext2d>().ok());

        if let Some(ctx) = ctx {
            let now = now_ms();
            draw_board(&ctx, &g.board, selected.as_deref(), &targets, last_move, anim.as_ref(), now);
        }
    });

    // Animation loop via requestAnimationFrame
    Effect::new(move || {
        let anim = animation.get();
        if anim.is_none() {
            return;
        }

        let Some(canvas) = canvas_ref.get() else { return };
        let canvas_el: &web_sys::HtmlCanvasElement = canvas.as_ref();
        let ctx = canvas_el
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|obj| obj.dyn_into::<web_sys::CanvasRenderingContext2d>().ok());
        let Some(ctx) = ctx else { return };

        let window = web_sys::window().unwrap();
        let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::new(move || {
            let now = now_ms();

            let anim_state = animation.get_untracked();
            let Some(ref a) = anim_state else {
                // Animation was cleared externally
                return;
            };

            let t = ((now - a.start_time) / a.duration_ms).clamp(0.0, 1.0);

            // Redraw board with animation
            let game_state = game.get_untracked();
            if let Some(ref gs) = game_state {
                let selected = selected_square.get_untracked();
                let moves = legal_moves.get_untracked();
                let targets: HashSet<String> = moves.iter().map(|m| m.to.clone()).collect();
                let last_move = gs.move_history.last();
                draw_board(&ctx, &gs.board, selected.as_deref(), &targets, last_move, anim_state.as_ref(), now);
            }

            if t >= 1.0 {
                animation.set(None);
                // Drop the closure
                let _ = f.borrow_mut().take();
                return;
            }

            // Request next frame
            let _ = web_sys::window().unwrap().request_animation_frame(
                f.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            );
        }));

        let _ = window.request_animation_frame(
            g.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
        );
    });

    let on_canvas_click = move |ev: web_sys::MouseEvent| {
        if !is_my_turn.get() || move_in_progress.get() || ai_thinking.get() {
            return;
        }
        if pending_promotion.get().is_some() {
            return;
        }

        let Some(canvas) = canvas_ref.get() else { return };
        let canvas_el: &web_sys::HtmlCanvasElement = canvas.as_ref();
        let rect = canvas_el.get_bounding_client_rect();
        let x = ev.client_x() as f64 - rect.left();
        let y = ev.client_y() as f64 - rect.top();

        let Some(clicked_sq) = pixel_to_square(x, y) else { return };

        let Some(g) = game.get() else { return };
        let active_color = g.active_color.as_deref().unwrap_or("");

        let current_selected = selected_square.get();

        if let Some(ref sel) = current_selected {
            // A piece is already selected
            let moves = legal_moves.get();
            let matching_moves: Vec<&ChessMove> = moves
                .iter()
                .filter(|m| m.to == clicked_sq)
                .collect();

            if !matching_moves.is_empty() {
                // Clicked a legal move target
                let has_promotion = matching_moves.iter().any(|m| m.promotion.is_some());
                if has_promotion {
                    pending_promotion.set(Some((sel.clone(), clicked_sq)));
                } else {
                    let from = sel.clone();
                    let to = clicked_sq;
                    let gid = game_id_for_click.clone();
                    let moving_piece = g.board.pieces.get(&from).cloned();
                    send_move(
                        gid, from, to, None,
                        set_game, selected_square, legal_moves,
                        move_in_progress, set_ai_thinking,
                        moving_piece, animation,
                    );
                }
                return;
            }

            if &clicked_sq == sel {
                // Clicked same square - deselect
                selected_square.set(None);
                legal_moves.set(vec![]);
                return;
            }

            // Check if clicked another own piece
            if let Some(piece) = g.board.pieces.get(&clicked_sq) {
                if piece.color == active_color {
                    selected_square.set(Some(clicked_sq.clone()));
                    legal_moves.set(vec![]);
                    let gid = game_id_for_click.clone();
                    fetch_legal_moves(gid, clicked_sq, legal_moves);
                    return;
                }
            }

            // Clicked empty square / opponent piece that's not a legal target - deselect
            selected_square.set(None);
            legal_moves.set(vec![]);
        } else {
            // No piece selected - try to select one
            if let Some(piece) = g.board.pieces.get(&clicked_sq) {
                if piece.color == active_color {
                    selected_square.set(Some(clicked_sq.clone()));
                    legal_moves.set(vec![]);
                    let gid = game_id_for_click.clone();
                    fetch_legal_moves(gid, clicked_sq, legal_moves);
                }
            }
        }
    };

    let cursor_style = move || {
        if is_my_turn.get() && !move_in_progress.get() && !ai_thinking.get() {
            "border: 1px solid #333; display: block; cursor: pointer;"
        } else {
            "border: 1px solid #333; display: block; cursor: default;"
        }
    };

    view! {
        <div style="position: relative; display: inline-block;">
            <canvas
                node_ref=canvas_ref
                width="480"
                height="480"
                style=cursor_style
                on:click=on_canvas_click
            />
            {move || pending_promotion.get().map(|(from, to)| {
                let pieces = vec![
                    ("queen", "Q"),
                    ("rook", "R"),
                    ("bishop", "B"),
                    ("knight", "N"),
                ];
                view! {
                    <div style="position: absolute; top: 0; left: 0; right: 0; bottom: 0; \
                                background: rgba(0,0,0,0.3); display: flex; align-items: center; \
                                justify-content: center; z-index: 10;">
                        <div style="background: white; border: 2px solid #333; border-radius: 8px; \
                                    padding: 1em; display: flex; gap: 0.5em; flex-direction: column; \
                                    align-items: center;">
                            <strong>"Promote to:"</strong>
                            <div style="display: flex; gap: 0.5em;">
                                {pieces.into_iter().map(|(piece_name, letter)| {
                                    let from = from.clone();
                                    let to = to.clone();
                                    let piece_name = piece_name.to_string();
                                    let gid = game_id_for_promo.clone();
                                    view! {
                                        <button
                                            on:click=move |_| {
                                                pending_promotion.set(None);
                                                let moving_piece = game.get()
                                                    .and_then(|g| g.board.pieces.get(&from).cloned());
                                                send_move(
                                                    gid.clone(),
                                                    from.clone(),
                                                    to.clone(),
                                                    Some(piece_name.clone()),
                                                    set_game,
                                                    selected_square,
                                                    legal_moves,
                                                    move_in_progress,
                                                    set_ai_thinking,
                                                    moving_piece,
                                                    animation,
                                                );
                                            }
                                            style="font-size: 1.5em; padding: 0.5em 0.75em; \
                                                   cursor: pointer; border: 1px solid #999; \
                                                   border-radius: 4px; background: #f8f8f8;"
                                        >
                                            {letter}
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

#[component]
fn MoveHistoryTable(move_history: Vec<ChessMove>) -> impl IntoView {
    let rows: Vec<_> = move_history
        .iter()
        .enumerate()
        .rev()
        .map(|(i, m)| {
            let move_num = (i / 2) + 1;
            let is_white = i % 2 == 0;
            let color_label = if is_white { "W" } else { "B" };
            let notation = match &m.promotion {
                Some(p) => format!("{} \u{2192} {} ={}", m.from, m.to, p.chars().next().unwrap_or('?').to_uppercase()),
                None => format!("{} \u{2192} {}", m.from, m.to),
            };
            let bg = if i % 2 == 0 { "#f8f8f8" } else { "#ffffff" };
            view! {
                <tr style=format!("background: {bg};")>
                    <td style="padding: 2px 8px; text-align: right; color: #888;">{move_num}"."</td>
                    <td style="padding: 2px 8px; font-weight: bold;">{color_label}</td>
                    <td style="padding: 2px 8px; font-family: monospace;">{notation}</td>
                </tr>
            }
        })
        .collect();

    view! {
        <div style="max-height: 200px; overflow-y: auto; width: 480px; border: 1px solid #ccc; \
                    border-radius: 4px; margin-top: 0.5em;">
            <table style="width: 100%; border-collapse: collapse; font-size: 0.9em;">
                <thead>
                    <tr style="background: #e8e8e8; position: sticky; top: 0;">
                        <th style="padding: 4px 8px; text-align: right;">"#"</th>
                        <th style="padding: 4px 8px; text-align: left;">" "</th>
                        <th style="padding: 4px 8px; text-align: left;">"Move"</th>
                    </tr>
                </thead>
                <tbody>
                    {rows}
                </tbody>
            </table>
        </div>
    }
}

fn fetch_legal_moves(game_id: String, square: String, legal_moves: RwSignal<Vec<ChessMove>>) {
    let url = format!("/api/games/{game_id}/legal-moves");
    leptos::task::spawn_local(async move {
        let Ok(resp) = crate::api::get(&url).send().await else {
            return;
        };
        if let Ok(all_moves) = resp.json::<Vec<ChessMove>>().await {
            let filtered: Vec<ChessMove> = all_moves
                .into_iter()
                .filter(|m| m.from == square)
                .collect();
            legal_moves.set(filtered);
        }
    });
}

fn send_move(
    game_id: String,
    from: String,
    to: String,
    promotion: Option<String>,
    set_game: WriteSignal<Option<Game>>,
    selected_square: RwSignal<Option<String>>,
    legal_moves: RwSignal<Vec<ChessMove>>,
    move_in_progress: RwSignal<bool>,
    set_ai_thinking: WriteSignal<bool>,
    moving_piece: Option<Piece>,
    animation: RwSignal<Option<AnimationState>>,
) {
    // Start animation before the async request
    if let Some(piece) = moving_piece {
        if let (Some(from_px), Some(to_px)) = (square_to_center_px(&from), square_to_center_px(&to)) {
            let dur = animation_duration(&from, &to);
            let now = now_ms();
            animation.set(Some(AnimationState {
                piece,
                from_px,
                to_px,
                start_time: now,
                duration_ms: dur,
                moving_to: to.clone(),
            }));
        }
    }
    move_in_progress.set(true);
    selected_square.set(None);
    legal_moves.set(vec![]);

    let url = format!("/api/games/{game_id}/moves");
    let body = ChessMove {
        from,
        to,
        promotion,
    };

    leptos::task::spawn_local(async move {
        let result = crate::api::post(&url)
            .body(serde_json::to_string(&body).unwrap())
            .unwrap()
            .send()
            .await;

        move_in_progress.set(false);

        match result {
            Ok(resp) if resp.ok() => {
                if let Ok(updated_game) = resp.json::<Game>().await {
                    set_ai_thinking.set(updated_game.ai_thinking.unwrap_or(false));
                    set_game.set(Some(updated_game));
                }
            }
            Ok(resp) => {
                // Could show error toast; for now log it
                let _ = resp.text().await;
            }
            Err(_) => {}
        }
    });
}
