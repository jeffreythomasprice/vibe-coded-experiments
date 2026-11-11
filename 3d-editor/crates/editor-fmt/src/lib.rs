mod error;
mod file;
mod read;
mod write;

pub use error::LoadError;
pub use file::{FileError, load_file, save_file, write_atomic};
pub use read::read;
pub use write::write;

pub const FORMAT: &str = "scene";
pub const VERSION: u32 = 2;

fn migrate(_root: &mut toml::Table, from: u32) {
    debug_assert!(from <= VERSION);
}
