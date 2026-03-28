include!(concat!(env!("OUT_DIR"), "/codegen.rs"));

pub mod schemas {
    pub const PLAYER: &str = include_str!("../schemas/player.json");
    pub const BOARD_STATE: &str = include_str!("../schemas/board_state.json");
    pub const MOVE: &str = include_str!("../schemas/move.json");
    pub const GAME: &str = include_str!("../schemas/game.json");
    pub const LOGIN_REQUEST: &str = include_str!("../schemas/login_request.json");
    pub const LOGIN_RESPONSE: &str = include_str!("../schemas/login_response.json");
}
