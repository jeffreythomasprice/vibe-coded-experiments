include!(concat!(env!("OUT_DIR"), "/codegen.rs"));

pub mod schemas {
    pub const PLAYER: &str = include_str!("../schemas/player.json");
    pub const BOARD_STATE: &str = include_str!("../schemas/board_state.json");
    pub const MOVE: &str = include_str!("../schemas/move.json");
    pub const GAME: &str = include_str!("../schemas/game.json");
    pub const LOGIN_REQUEST: &str = include_str!("../schemas/login_request.json");
    pub const LOGIN_RESPONSE: &str = include_str!("../schemas/login_response.json");
    pub const USER_SUMMARY: &str = include_str!("../schemas/user_summary.json");
    pub const CREATE_USER_REQUEST: &str = include_str!("../schemas/create_user_request.json");
    pub const UPDATE_USER_REQUEST: &str = include_str!("../schemas/update_user_request.json");
    pub const LIST_USERS_RESPONSE: &str = include_str!("../schemas/list_users_response.json");
    pub const CREATE_GAME_REQUEST: &str = include_str!("../schemas/create_game_request.json");
    pub const GAME_SUMMARY: &str = include_str!("../schemas/game_summary.json");
    pub const LIST_GAMES_RESPONSE: &str = include_str!("../schemas/list_games_response.json");
    pub const AI_ENGINES_RESPONSE: &str = include_str!("../schemas/ai_engines_response.json");
}
