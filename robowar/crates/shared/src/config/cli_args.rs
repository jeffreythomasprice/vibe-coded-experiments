use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct LogArgs {
    /// Directory for log files
    #[arg(long)]
    pub log_dir: Option<PathBuf>,

    /// Disable file logging
    #[arg(long)]
    pub no_file_log: bool,
}

#[derive(Debug, clap::Args)]
pub struct MatchArgs {
    /// Robot config files (.toml)
    #[arg()]
    pub robots: Vec<PathBuf>,

    /// Arena config file (.toml)
    #[arg(long)]
    pub arena: Option<PathBuf>,

    /// Maximum ticks
    #[arg(long)]
    pub ticks: Option<u32>,

    /// RNG seed
    #[arg(long)]
    pub seed: Option<u64>,

    /// Project config file (default: ./robowar.toml)
    #[arg(long, default_value = "robowar.toml")]
    pub config: PathBuf,
}
