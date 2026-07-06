mod app;
mod catalog;
mod egui_platform;
mod gfx;
mod input;
mod pace;
mod rom;
mod snd;
mod storage;

use std::path::PathBuf;

use anyhow::{bail, Context};
use common::{RomLibrary, SpeedMode};
use gameboy::Cartridge;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::App;
use crate::catalog::RomCatalog;
use crate::storage::FileStore;

/// Usage text emitted by `--help`/`-h`.
const HELP: &str = "\
emulator — a Game Boy emulator shell

Usage:
  emulator [--config <path>] [--speed <s>]           Launch the shell in menu mode (pick a ROM in-app)
  emulator run <name> [--config <path>] [--speed <s>] Boot the ROM matching <name> (file name or header title)
  emulator list-roms [--config <path>]               List every ROM found in the roms dir (scanned recursively)
  emulator print-config                              Print the default settings.toml and exit
  emulator --help                                    Show this help and exit

Options:
  -c, --config <path>   Use an alternate settings file instead of the default location.
  -s, --speed <s>       Emulation speed for this run: a positive factor (e.g. 1, 2, 3.75)
                        or \"max\" for as-fast-as-possible. Overrides the saved setting;
                        not persisted. Defaults to the saved setting (1x).
  -h, --help            Show this help and exit.
";

/// The requested command. The default (no subcommand) launches the menu UI; the
/// other variants cover `run <name>`, `print-config`, and `--help`.
enum Cli {
    Help,
    PrintConfig,
    ListRoms {
        config: Option<PathBuf>,
    },
    Run {
        config: Option<PathBuf>,
        name: String,
        speed: Option<SpeedMode>,
    },
    Menu {
        config: Option<PathBuf>,
        speed: Option<SpeedMode>,
    },
}

/// Minimal hand-rolled parser (no clap): `--help`/`-h` prints help and exits;
/// `print-config` prints the default settings; `run <name>` boots a ROM by name;
/// no subcommand launches the shell in menu mode. `--config`/`-c`/`--config=`
/// selects the settings file for the run/menu commands.
fn parse_args() -> anyhow::Result<Cli> {
    let mut config = None;
    let mut speed = None;
    let mut subcommand: Option<String> = None;
    let mut name: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            return Ok(Cli::Help);
        } else if arg == "--config" || arg == "-c" {
            let value = args.next().context("--config requires a path argument")?;
            config = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--config=") {
            config = Some(PathBuf::from(value));
        } else if arg == "--speed" || arg == "-s" {
            let value = args.next().context("--speed requires a value argument")?;
            speed = Some(SpeedMode::parse_cli(&value).map_err(|err| anyhow::anyhow!(err))?);
        } else if let Some(value) = arg.strip_prefix("--speed=") {
            speed = Some(SpeedMode::parse_cli(value).map_err(|err| anyhow::anyhow!(err))?);
        } else if arg.starts_with('-') && arg != "-" {
            bail!("unknown argument: {arg}");
        } else if subcommand.is_none() {
            subcommand = Some(arg);
        } else if subcommand.as_deref() == Some("run") && name.is_none() {
            name = Some(arg);
        } else {
            bail!("unexpected extra argument: {arg}");
        }
    }

    match subcommand.as_deref() {
        None => Ok(Cli::Menu { config, speed }),
        Some("print-config") => Ok(Cli::PrintConfig),
        Some("list-roms") => Ok(Cli::ListRoms { config }),
        Some("run") => {
            let name = name.context("`run` requires a rom name argument")?;
            Ok(Cli::Run {
                config,
                name,
                speed,
            })
        }
        Some(other) => bail!("unknown subcommand: {other}"),
    }
}

fn main() -> anyhow::Result<()> {
    common::init().context("failed to initialize logging")?;

    match parse_args()? {
        Cli::Help => {
            print!("{HELP}");
            Ok(())
        }
        Cli::PrintConfig => {
            let text = FileStore::default_config_toml()
                .context("failed to render the default configuration")?;
            print!("{text}");
            Ok(())
        }
        Cli::ListRoms { config } => list_roms(config),
        Cli::Menu { config, speed } => run_menu(config, speed),
        Cli::Run {
            config,
            name,
            speed,
        } => run_rom(config, &name, speed),
    }
}

/// Print every ROM in the configured roms dir (scanned recursively), one per line
/// as its path relative to the roms dir, annotated with the cartridge header title
/// when the ROM carries one. Prints to stdout — it is program output, not a log.
fn list_roms(config: Option<PathBuf>) -> anyhow::Result<()> {
    let store = FileStore::open(config).context("failed to open persistent-state store")?;
    let catalog = RomCatalog::load(&store);
    let roms = catalog.roms();
    if roms.is_empty() {
        println!("no roms found in the roms dir");
        return Ok(());
    }
    for info in roms {
        match info.title() {
            "" => println!("{}", info.id.0),
            title => println!("{}  ({title})", info.id.0),
        }
    }
    Ok(())
}

/// The default command: launch the emulator window with no ROM loaded, showing
/// the menu overlay. `App` scans the ROM library itself, so a ROM can be picked
/// from the in-app list.
fn run_menu(config: Option<PathBuf>, speed: Option<SpeedMode>) -> anyhow::Result<()> {
    let store = FileStore::open(config).context("failed to open persistent-state store")?;
    tracing::info!("starting emulator shell in menu mode");
    run_app(store, None, None, speed)
}

/// Boot a ROM found by name (file name or header title) in the emulator window.
fn run_rom(config: Option<PathBuf>, name: &str, speed: Option<SpeedMode>) -> anyhow::Result<()> {
    let store = FileStore::open(config).context("failed to open persistent-state store")?;
    let catalog = RomCatalog::load(&store);

    let info = match catalog.find(name) {
        Ok(Some(info)) => info,
        Ok(None) => bail!("no rom matching {name:?} in the roms dir"),
        Err(candidates) => {
            let names: Vec<&str> = candidates.iter().map(|c| c.file_stem.as_str()).collect();
            bail!("{name:?} is ambiguous; matched: {}", names.join(", "));
        }
    };

    let bytes = store
        .read_rom(&info.id)
        .with_context(|| format!("failed to read rom {}", info.id.0))?;
    let mut cart = Cartridge::from_bytes(&bytes)
        .with_context(|| format!("failed to parse rom {}", info.id.0))?;
    let save_id = rom::save_id_for(&cart, &info.file_stem);
    rom::restore_battery(&mut cart, &save_id, &store)?;

    tracing::info!(rom = %info.id.0, "starting emulator shell");
    run_app(store, Some(cart), Some(save_id), speed)
}

/// Build the winit event loop and run the `App`, optionally with a loaded cartridge
/// and a speed override (which wins over the persisted setting for this run).
fn run_app(
    store: FileStore,
    cartridge: Option<Cartridge>,
    save_id: Option<common::SaveId>,
    speed: Option<SpeedMode>,
) -> anyhow::Result<()> {
    let scale_mode = store.settings().graphics.scale_mode;

    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(store, cartridge, save_id, scale_mode, speed);
    event_loop.run_app(&mut app).context("event loop error")?;

    Ok(())
}
