include!(concat!(env!("OUT_DIR"), "/codegen.rs"));

pub mod schemas {
    pub const CREATE_GAME: &str = include_str!("../schemas/create_game.json");
}
