use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod ai;
mod auth;
mod db;
mod extractors;
mod routes;

use ai::manager::AiManager;
use ai::registry::AiRegistry;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
    pub ai_manager: Arc<AiManager>,
}

#[tokio::main]
async fn main() {
    let server_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    dotenvy::from_path(server_dir.join(".env")).ok();
    dotenvy::from_path(server_dir.join(".secrets")).ok();

    let jwt_secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set — see CLAUDE.md for generation instructions");

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chess_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let pool = db::init_pool().await;

    let registry = Arc::new(AiRegistry::new());
    let ai_manager = Arc::new(AiManager::new(registry, pool.clone()));
    ai_manager.recover_thinking_games().await;

    let state = AppState {
        db: pool,
        jwt_secret,
        ai_manager,
    };

    let app = Router::new()
        .route("/login", post(routes::login::login))
        .route(
            "/users",
            post(routes::users::create_user).get(routes::users::list_users),
        )
        .route(
            "/users/{username}",
            get(routes::users::get_user)
                .put(routes::users::update_user)
                .delete(routes::users::delete_user),
        )
        .route(
            "/games",
            post(routes::games::create_game).get(routes::games::list_games),
        )
        .route(
            "/games/{game_id}",
            get(routes::games::get_game).delete(routes::games::delete_game),
        )
        .route(
            "/games/{game_id}/moves",
            post(routes::games::make_move),
        )
        .route(
            "/games/{game_id}/legal-moves",
            get(routes::games::legal_moves),
        )
        .route(
            "/games/{game_id}/abandon",
            post(routes::games::abandon_game),
        )
        .route(
            "/games/{game_id}/surrender",
            post(routes::games::surrender_game),
        )
        .route(
            "/games/{game_id}/ws",
            get(routes::ws::game_ws),
        )
        .route(
            "/me/theme",
            get(routes::theme::get_my_theme).put(routes::theme::update_my_theme),
        )
        .route("/ai-engines", get(routes::ai::list_engines))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:8001";
    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
