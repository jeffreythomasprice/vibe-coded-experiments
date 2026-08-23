mod commands;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use lib::config::Config;
use lib::service::Service;

/// Every model from every configured provider, merging Ollama's local
/// `/api/tags` list into its `ollama.com/library` index (see
/// `lib::catalog::merge_ollama`). Returns `Result` only because Tauri
/// requires it of an async command that borrows `State`; a per-provider
/// failure rides inside the payload's `ProviderModels::errors` rather than
/// failing this command outright, so one misconfigured provider never blanks
/// out the others.
#[tauri::command]
async fn model_catalog(config: tauri::State<'_, Config>) -> Result<shared::llm::ModelCatalog, String> {
    Ok(lib::catalog::load(&config).await)
}

/// Sink for log events forwarded from the wasm frontend, so client and server
/// logs end up interleaved in the same rotated files.
#[tauri::command]
fn log_from_client(record: shared::ClientLogRecord) {
    lib::logging::log_client_record(&record);
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // May run before the logger exists, so stderr is all we have.
            eprintln!("ai-harness: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    // Loaded before the config, since the config's `api_key_env` values name
    // variables this is meant to populate. A missing `.env` is fine — most
    // runs won't have one — but a present-and-unparsable one is reported the
    // same way a bad config file would be.
    let dotenv_source = load_dotenv()?;

    // Config must be read before the logger, because the config is what says
    // where the logger writes. Anything that fails up to `logging::init` can
    // only be reported on stderr.
    let (mut config, source) = Config::load(parse_config_flag())?;
    config.expand_paths()?;

    lib::logging::init(&config.log)?;

    // Now that the logger exists, record what we loaded and where from.
    match &dotenv_source {
        Some(path) => tracing::info!(dotenv_source = %path.display(), "loaded .env"),
        None => tracing::debug!("no .env file found"),
    }
    match &source {
        Some(path) => tracing::info!(config_source = %path.display(), "loaded config"),
        None => tracing::warn!(
            searched = ?lib::config::search_paths(),
            "no config file found; using built-in defaults"
        ),
    }
    match toml::to_string_pretty(&config) {
        Ok(dump) => tracing::info!("effective config:\n{}", dump.trim_end()),
        Err(err) => tracing::warn!(%err, "could not render effective config"),
    }

    // A `Service` opens the database and builds the LLM router — both do
    // real I/O, so this needs the async runtime, but `run()` itself stays
    // sync (Tauri's `App::run` never returns; there is no natural point to
    // `.await` after it starts). `tauri::async_runtime::block_on` runs this
    // to completion on the same runtime Tauri's own async commands use,
    // rather than spinning up a second one just for this one call.
    let service = tauri::async_runtime::block_on(Service::from_config(&config))
        .context("starting the local database and LLM router")?;

    tracing::info!("starting tauri");
    tauri::Builder::default()
        .manage(config.clone())
        .manage(service)
        .invoke_handler(tauri::generate_handler![
            model_catalog,
            log_from_client,
            commands::agents::list_agents,
            commands::agents::get_agent,
            commands::agents::tool_catalog,
            commands::agents::create_agent,
            commands::agents::update_agent,
            commands::agents::delete_agent,
            commands::conversations::list_conversations,
            commands::conversations::create_conversation,
            commands::conversations::get_conversation,
            commands::conversations::rename_conversation,
            commands::conversations::delete_conversation,
            commands::conversations::send_message,
            commands::conversations::approve_tools,
            commands::conversations::cancel_pending,
            commands::preferences::get_preference,
            commands::preferences::set_preference,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}

/// Load a `.env` file from the working directory or any parent, if one
/// exists, into the process environment. Variables already set in the
/// environment take precedence over the file — see `.env.example`.
///
/// Returns the loaded file's path, or `None` if there simply wasn't one;
/// that's the common case, not an error. An `.env` that exists but fails to
/// parse *is* an error, same as a malformed config file.
fn load_dotenv() -> anyhow::Result<Option<PathBuf>> {
    match dotenvy::dotenv() {
        Ok(path) => Ok(Some(path)),
        Err(err) if err.not_found() => Ok(None),
        Err(err) => Err(err).context("loading .env file"),
    }
}

/// Pull `--config <path>` (or `--config=<path>`) out of argv.
///
/// Deliberately lenient rather than a real arg parser: this binary's argv also
/// carries whatever the Tauri CLI and the OS inject (`cargo tauri dev` adds its
/// own; macOS adds `-psn_*`), and a strict parser would reject those.
fn parse_config_flag() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
        if arg == "--config" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}
