use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "diffusion",
    version,
    about = "Generate images with diffusion-rs"
)]
pub struct Cli {
    /// Path to a config.toml, overriding the default search locations
    #[arg(short, long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}
