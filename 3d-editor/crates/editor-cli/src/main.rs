use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use base64::Engine;
use clap::{Parser, Subcommand};
use editor_cmd::results::{FileInfoResult, SessionAttached};
use editor_cmd::{CommandError, ErrorCode, Exec, OPS, OpInfo, Request, Response, Session};
use editor_core::Document;
use editor_fmt::FileError;
use editor_proto::{
    Client, ClientError, Config, ConfigError, ControlAction, NotRunning, ServerEvent,
};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    about = "Drive scene editor sessions through the editor-server",
    after_help = "Without a server: `editor-cli ops [--safety read|mutate|destructive] [--json]` lists commands, `editor-cli schema <op> [--result]` prints an op's argument (or response) schema, `editor-cli schema --wire` the socket protocol's."
)]
struct Cli {
    /// Scene file (.scene.toml) to attach to; default is the server's focused session
    #[arg(short, long, global = true)]
    file: Option<PathBuf>,

    /// Pretty-print the JSON response
    #[arg(long, global = true)]
    pretty: bool,

    /// Config file (default: $EDITOR_CONFIG or ~/.config/editor/config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Create an empty scene file (needs --file; does not use the server)
    New {
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing file
        #[arg(long)]
        force: bool,
    },
    /// Run one command
    Run {
        op: String,
        /// Command arguments as a JSON object
        #[arg(long, default_value = "{}")]
        args: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        expect_revision: Option<u64>,
        /// Save the file afterwards even if the session was already open elsewhere
        #[arg(long)]
        save: bool,
    },
    /// Render the session to a PNG (never modifies the scene)
    Screenshot {
        /// Camera id, name or viewport.persp|top|front|side
        #[arg(long)]
        camera: Option<String>,
        /// Output PNG path
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
        /// shaded, flat or wireframe
        #[arg(long)]
        mode: Option<String>,
        /// Comma-separated: grid, axes, labels, bounds, selection, links
        #[arg(long, value_delimiter = ',')]
        overlay: Vec<String>,
        /// Comma-separated object ids to highlight
        #[arg(long, value_delimiter = ',')]
        highlight: Vec<String>,
        /// Comma-separated ids, groups or layers to leave out
        #[arg(long, value_delimiter = ',')]
        hide: Vec<String>,
        /// Draw nothing above this world z (plan and section views)
        #[arg(long)]
        clip_above: Option<f64>,
        /// Comma-separated ids to restrict labels to
        #[arg(long, value_delimiter = ',')]
        label_ids: Vec<String>,
        #[arg(long)]
        max_labels: Option<usize>,
        /// material, orientation or layer
        #[arg(long)]
        colors: Option<String>,
        /// Colour override NAME=#rrggbb (repeatable)
        #[arg(long = "material-color")]
        material_color: Vec<String>,
        /// Full view.screenshot arguments as JSON; flags override it
        #[arg(long)]
        args: Option<String>,
    },
    /// Run a JSON array of {op, args} as one all-or-nothing batch
    Apply {
        batch: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        save: bool,
    },
    /// Read one JSON request per stdin line against the attached session
    Session {
        /// Save the file after every change
        #[arg(long)]
        autosave: bool,
        /// Refuse mutating and destructive ops
        #[arg(long)]
        read_only: bool,
        /// Also print change events as JSON lines
        #[arg(long)]
        events: bool,
        /// Include the full document in snapshot events
        #[arg(long)]
        full: bool,
    },
    /// Print change events from the attached session until the server closes
    Watch {
        /// Include the full document in snapshot events
        #[arg(long)]
        full: bool,
    },
    /// List the server's sessions
    Sessions,
    /// Inspect or stop the server
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
}

#[derive(Subcommand)]
enum ServerAction {
    /// Show connections and sessions
    Status,
    /// Stop the server (refused while a session has unsaved changes unless --force)
    Stop {
        #[arg(long)]
        force: bool,
    },
}

#[derive(Parser)]
#[command(name = "editor-cli")]
struct MetaCli {
    #[command(subcommand)]
    command: Meta,
}

#[derive(Subcommand)]
enum Meta {
    /// List commands
    Ops {
        #[arg(long)]
        safety: Option<String>,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
    /// Print the JSON schema of an op's arguments, its result (--result), or of the socket protocol (--wire)
    Schema {
        #[arg(required_unless_present = "wire")]
        op: Option<String>,
        /// The schema of the whole response (`ok`, `revision`, `result`, ...) instead of the arguments
        #[arg(long, conflicts_with = "wire")]
        result: bool,
        /// The schemas of every message on the server socket
        #[arg(long)]
        wire: bool,
    },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}")]
    File(#[from] FileError),
    #[error("{what}: {source}")]
    Json {
        what: &'static str,
        source: serde_json::Error,
    },
    #[error("{0}")]
    Usage(String),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Config(#[from] ConfigError),
}

impl CliError {
    fn to_command_error(&self) -> CommandError {
        match self {
            CliError::Io { .. } | CliError::File(FileError::Io { .. }) | CliError::Config(_) => {
                CommandError::Io {
                    message: self.to_string(),
                }
            }
            CliError::File(FileError::Load { .. }) => CommandError::Format {
                message: self.to_string(),
            },
            CliError::Client(e) => match e {
                ClientError::Protocol(message) => CommandError::Protocol {
                    message: message.clone(),
                },
                other => CommandError::ServerUnavailable {
                    message: other.to_string(),
                },
            },
            CliError::Json { .. } | CliError::Usage(_) => CommandError::InvalidArgument {
                field: None,
                message: self.to_string(),
                hint: None,
            },
        }
    }
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> CliError + '_ {
    |source| CliError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn parse_json(what: &'static str, text: &str) -> Result<Value, CliError> {
    serde_json::from_str(text).map_err(|source| CliError::Json { what, source })
}

fn exit_code(value: &Value) -> u8 {
    if value["ok"] == true {
        return 0;
    }
    match serde_json::from_value::<ErrorCode>(value["code"].clone()) {
        Ok(
            ErrorCode::Io
            | ErrorCode::Format
            | ErrorCode::Render
            | ErrorCode::ServerUnavailable
            | ErrorCode::Protocol,
        ) => 2,
        _ => 1,
    }
}

fn error_value(e: &CliError) -> Value {
    Response::error(&e.to_command_error()).to_value()
}

fn load_config(cli: &Cli) -> Result<Config, CliError> {
    Ok(match &cli.config {
        Some(path) => Config::load_from(path)?,
        None => Config::load()?,
    })
}

fn connect(cli: &Cli, writable: bool, autospawn: bool) -> Result<Client, CliError> {
    let config = load_config(cli)?;
    Ok(Client::connect_with(
        &config, "cli", writable, writable, autospawn, None,
    )?)
}

/// Attaches to `--file`, or to the focused session. Ok(Err(response)) is a refusal to print as is.
fn attach(client: &Client, file: Option<&Path>) -> Result<Result<bool, Value>, CliError> {
    let args = match file {
        Some(path) => json!({ "path": path }),
        None => json!({}),
    };
    let reply = client.request_value(json!({ "op": "session.attach", "args": args }))?;
    if !reply.is_ok() {
        return Ok(Err(reply.value()));
    }
    Ok(Ok(reply.result::<SessionAttached>()?.created))
}

fn save_if_dirty(client: &Client) -> Result<Option<Value>, CliError> {
    let info = client.request_value(json!({ "op": "file.info" }))?;
    if !info.result::<FileInfoResult>()?.dirty {
        return Ok(None);
    }
    let saved = client.request_value(json!({ "op": "file.save" }))?;
    Ok((!saved.is_ok()).then(|| saved.value()))
}

fn one_shot(cli: &Cli, request: Request, save: bool) -> Result<Value, CliError> {
    let client = connect(cli, true, true)?;
    client.discard_events();
    let created = match attach(&client, cli.file.as_deref())? {
        Ok(created) => created,
        Err(refusal) => return Ok(refusal),
    };
    if created && cli.file.is_none() {
        return Ok(Response::error(
            &CommandError::invalid_arg("file", "no session is open on the server")
                .with_hint("pass --file, or open a scene in the editor first"),
        )
        .to_value());
    }
    let dry_run = request.dry_run;
    let reply = client.request(request)?;
    let value = reply.value();
    if value["ok"] == true
        && !dry_run
        && (created || save)
        && let Some(failure) = save_if_dirty(&client)?
    {
        return Ok(failure);
    }
    Ok(value)
}

struct ScreenshotFlags<'a> {
    camera: &'a Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    mode: &'a Option<String>,
    overlay: &'a [String],
    highlight: &'a [String],
    hide: &'a [String],
    clip_above: Option<f64>,
    label_ids: &'a [String],
    max_labels: Option<usize>,
    colors: &'a Option<String>,
    material_color: &'a [String],
}

fn screenshot_args(base: &Option<String>, flags: ScreenshotFlags) -> Result<Value, CliError> {
    let mut args = match base {
        Some(text) => parse_json("--args", text)?,
        None => json!({}),
    };
    let Some(map) = args.as_object_mut() else {
        return Err(CliError::Usage("--args must be a JSON object".into()));
    };
    if let Some(camera) = flags.camera {
        map.insert("camera".into(), json!(camera));
    }
    if let Some(w) = flags.width {
        map.insert("width".into(), json!(w));
    }
    if let Some(h) = flags.height {
        map.insert("height".into(), json!(h));
    }
    if let Some(mode) = flags.mode {
        map.insert("mode".into(), json!(mode));
    }
    if !flags.overlay.is_empty() {
        map.insert("overlays".into(), json!(flags.overlay));
    }
    if !flags.highlight.is_empty() {
        map.insert("highlight".into(), json!(flags.highlight));
    }
    if !flags.hide.is_empty() {
        map.insert("hide".into(), json!(flags.hide));
    }
    if let Some(z) = flags.clip_above {
        map.insert("clip_above".into(), json!(z));
    }
    if !flags.label_ids.is_empty() {
        map.insert("label_ids".into(), json!(flags.label_ids));
    }
    if let Some(n) = flags.max_labels {
        map.insert("max_labels".into(), json!(n));
    }
    if let Some(colors) = flags.colors {
        map.insert("colors".into(), json!(colors));
    }
    if !flags.material_color.is_empty() {
        let mut overrides = map
            .remove("material_colors")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        for entry in flags.material_color {
            let (name, color) = entry.split_once('=').ok_or_else(|| {
                CliError::Usage(format!(
                    "--material-color expects NAME=#rrggbb, got `{entry}`"
                ))
            })?;
            overrides.insert(name.to_string(), json!(color));
        }
        map.insert("material_colors".into(), Value::Object(overrides));
    }
    Ok(args)
}

fn run(cli: &Cli) -> Result<Value, CliError> {
    match &cli.command {
        Cmd::New { name, force } => {
            let file = cli
                .file
                .as_ref()
                .ok_or_else(|| CliError::Usage("new needs --file".into()))?;
            if file.exists() && !force {
                return Err(CliError::Io {
                    path: file.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "file exists (use --force to overwrite)",
                    ),
                });
            }
            let default_name = file
                .file_name()
                .map(|n| {
                    n.to_string_lossy()
                        .trim_end_matches(".scene.toml")
                        .to_string()
                })
                .unwrap_or_default();
            let doc = Document::new(name.clone().unwrap_or(default_name));
            editor_fmt::save_file(file, &doc)?;
            let mut session = Session::new(doc);
            Ok(session
                .execute(Request::new("scene.summary", json!({})), "cli")
                .to_value())
        }
        Cmd::Run {
            op,
            args,
            dry_run,
            expect_revision,
            save,
        } => {
            let mut request = Request::new(op.clone(), parse_json("--args", args)?);
            request.dry_run = *dry_run;
            request.expect_revision = *expect_revision;
            one_shot(cli, request, *save)
        }
        Cmd::Screenshot {
            camera,
            out,
            width,
            height,
            mode,
            overlay,
            highlight,
            hide,
            clip_above,
            label_ids,
            max_labels,
            colors,
            material_color,
            args,
        } => {
            let mut args = screenshot_args(
                args,
                ScreenshotFlags {
                    camera,
                    width: *width,
                    height: *height,
                    mode,
                    overlay,
                    highlight,
                    hide,
                    clip_above: *clip_above,
                    label_ids,
                    max_labels: *max_labels,
                    colors,
                    material_color,
                },
            )?;
            args["path"] = json!(out);
            one_shot(cli, Request::new("view.screenshot", args), false)
        }
        Cmd::Apply {
            batch,
            dry_run,
            save,
        } => {
            let text = fs::read_to_string(batch).map_err(io_err(batch))?;
            let value = parse_json("batch file", &text)?;
            let commands = match value {
                Value::Array(_) => value,
                Value::Object(mut map) => map.remove("commands").ok_or_else(|| {
                    CliError::Usage("batch file must be an array or have a `commands` key".into())
                })?,
                _ => {
                    return Err(CliError::Usage(
                        "batch file must be a JSON array of {op, args}".into(),
                    ));
                }
            };
            let mut request = Request::new("batch", json!({ "commands": commands }));
            request.dry_run = *dry_run;
            one_shot(cli, request, *save)
        }
        Cmd::Sessions => {
            let client = connect(cli, false, true)?;
            client.discard_events();
            Ok(client
                .request_value(json!({ "op": "session.list" }))?
                .value())
        }
        Cmd::Server { action } => server_action(cli, action),
        Cmd::Session { .. } | Cmd::Watch { .. } => unreachable!("handled in main"),
    }
}

fn server_action(cli: &Cli, action: &ServerAction) -> Result<Value, CliError> {
    let client = match connect(cli, true, false) {
        Ok(c) => c,
        Err(CliError::Client(ClientError::Connect { .. })) => {
            return Ok(json!({ "ok": true, "result": NotRunning { running: false } }));
        }
        Err(e) => return Err(e),
    };
    client.discard_events();
    let reply = match action {
        ServerAction::Status => client.control(ControlAction::Status, false)?,
        ServerAction::Stop { force } => client.control(ControlAction::Stop, *force)?,
    };
    Ok(reply.value())
}

fn event_value(event: &ServerEvent, full: bool) -> Value {
    let (name, mut value) = match event {
        ServerEvent::Snapshot(s) => (
            "snapshot",
            serde_json::to_value(s).expect("events serialize"),
        ),
        ServerEvent::Changed(c) => (
            "changed",
            serde_json::to_value(c).expect("events serialize"),
        ),
        ServerEvent::Closed => ("closed", json!({})),
    };
    if let Some(map) = value.as_object_mut() {
        map.insert("event".into(), json!(name));
        if !full {
            map.remove("document_toml");
        }
    }
    value
}

fn print_events(client: &Client, full: bool, stop: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    loop {
        match client.next_event(Duration::from_millis(100)) {
            Some(event) => {
                let closed = matches!(event, ServerEvent::Closed);
                let mut out = std::io::stdout().lock();
                let line = event_value(&event, full).to_string();
                if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() || closed {
                    return;
                }
            }
            None if stop.load(Ordering::Relaxed) => return,
            None => {}
        }
    }
}

fn emit(value: &Value) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{value}")?;
    out.flush()
}

fn session_reply(client: &Client, line: &str) -> Result<Value, CliError> {
    let request = match serde_json::from_str::<Value>(line) {
        Ok(v) => v,
        Err(e) => {
            return Ok(Response::error(&CommandError::InvalidArgument {
                field: None,
                message: format!("not valid JSON: {e}"),
                hint: Some("send one {\"op\": ..., \"args\": ...} object per line".into()),
            })
            .to_value());
        }
    };
    let request = match Request::from_value(request) {
        Ok(r) => r,
        Err(e) => return Ok(Response::error(&e).to_value()),
    };
    let reply = client.request(request)?;
    let mut value = reply.value();
    if let Some(png) = reply.png
        && value["result"].get("path").is_none()
    {
        value["result"]["png_base64"] =
            json!(base64::engine::general_purpose::STANDARD.encode(png));
    }
    Ok(value)
}

fn session_mode(
    cli: &Cli,
    autosave: bool,
    read_only: bool,
    events: bool,
    full: bool,
) -> Result<(), CliError> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let client = connect(cli, !read_only, true)?;
    if !events {
        client.discard_events();
    }
    if let Err(refusal) = attach(&client, cli.file.as_deref())? {
        emit(&refusal).ok();
        return Ok(());
    }
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| -> Result<(), CliError> {
        if events {
            scope.spawn(|| print_events(&client, full, &stop));
        }
        let result = (|| {
            for line in std::io::stdin().lock().lines() {
                let line = line.map_err(io_err(Path::new("<stdin>")))?;
                if line.trim().is_empty() {
                    continue;
                }
                let value = session_reply(&client, &line)?;
                emit(&value).map_err(io_err(Path::new("<stdout>")))?;
                if autosave && let Some(failure) = save_if_dirty(&client)? {
                    eprintln!("autosave failed: {failure}");
                }
            }
            Ok(())
        })();
        std::thread::sleep(Duration::from_millis(if events { 150 } else { 0 }));
        stop.store(true, Ordering::Relaxed);
        result
    })?;
    if let Ok(info) = client.request_value(json!({ "op": "file.info" }))
        && info.result::<FileInfoResult>().is_ok_and(|i| i.dirty)
    {
        eprintln!(
            "warning: the session has unsaved changes (kept by editor-server; use file.save or --autosave)"
        );
    }
    Ok(())
}

fn watch_mode(cli: &Cli, full: bool) -> Result<(), CliError> {
    let client = connect(cli, false, true)?;
    if let Err(refusal) = attach(&client, cli.file.as_deref())? {
        emit(&refusal).ok();
        return Ok(());
    }
    let stop = std::sync::atomic::AtomicBool::new(false);
    print_events(&client, full, &stop);
    Ok(())
}

fn main() -> ExitCode {
    if matches!(std::env::args().nth(1).as_deref(), Some("ops" | "schema")) {
        return meta(MetaCli::parse());
    }
    let cli = Cli::parse();
    let outcome = match &cli.command {
        Cmd::Session {
            autosave,
            read_only,
            events,
            full,
        } => session_mode(&cli, *autosave, *read_only, *events, *full).map(|()| None),
        Cmd::Watch { full } => watch_mode(&cli, *full).map(|()| None),
        _ => run(&cli).map(Some),
    };
    match outcome {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(value)) => {
            print_value(&value, cli.pretty);
            ExitCode::from(exit_code(&value))
        }
        Err(e) => {
            print_value(&error_value(&e), cli.pretty);
            ExitCode::from(2)
        }
    }
}

fn print_value(value: &Value, pretty: bool) {
    let text = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .expect("responses serialize");
    println!("{text}");
}

fn meta(cli: MetaCli) -> ExitCode {
    match cli.command {
        Meta::Ops { safety, json } => {
            let mut ops: Vec<&OpInfo> = OPS.iter().collect();
            if let Some(wanted) = &safety {
                ops.retain(|o| o.safety.as_str() == wanted);
            }
            ops.sort_by_key(|o| o.name);
            if json {
                let list: Vec<Value> = ops
                    .iter()
                    .map(|o| {
                        json!({
                            "name": o.name,
                            "safety": o.safety.as_str(),
                            "scope": exec_name(o.exec),
                            "undoable": o.undoable(),
                            "summary": o.summary,
                        })
                    })
                    .collect();
                print_value(&json!({ "ops": list }), false);
            } else {
                for o in ops {
                    println!("{:<24} {:<12} {}", o.name, o.safety.as_str(), o.summary);
                }
            }
            ExitCode::SUCCESS
        }
        Meta::Schema { wire: true, .. } => {
            print_value(&editor_proto::wire_schema(), true);
            ExitCode::SUCCESS
        }
        Meta::Schema { op, result, .. } => {
            let op = op.unwrap_or_default();
            match OpInfo::find(&op) {
                Some(info) => {
                    let build = if result {
                        info.response_schema
                    } else {
                        info.args_schema
                    };
                    let schema = serde_json::to_value(build()).expect("schemas serialize");
                    print_value(&schema, true);
                    ExitCode::SUCCESS
                }
                None => {
                    let e = CommandError::invalid_arg("op", format!("unknown op `{op}`"))
                        .with_hint("run `editor-cli ops` for the list");
                    print_value(&Response::error(&e).to_value(), false);
                    ExitCode::from(1)
                }
            }
        }
    }
}

fn exec_name(exec: Exec) -> &'static str {
    match exec {
        Exec::Document => "document",
        Exec::Session => "session",
        Exec::Client => "client",
        Exec::Server => "server",
    }
}
