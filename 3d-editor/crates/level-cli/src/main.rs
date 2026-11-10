use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::Engine;
use clap::{Parser, Subcommand};
use level_cmd::{Changes, CommandError, Exec, OPS, OpInfo, Request, Response, Session, Success};
use level_core::Document;
use level_fmt::FileError;
use level_host::{Host, Policy};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    about = "Drive a level file through the command API",
    after_help = "Without a file: `level-cli ops [--safety read|mutate|destructive] [--json]` lists commands, `level-cli schema <op>` prints an op's argument schema."
)]
struct Cli {
    /// Level file (.level.toml)
    file: PathBuf,

    /// Pretty-print the JSON response
    #[arg(long, global = true)]
    pretty: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an empty level file
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
    },
    /// Render the level to a PNG (never modifies the level file)
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
        /// Comma-separated: grid, axes, labels, bounds
        #[arg(long, value_delimiter = ',')]
        overlay: Vec<String>,
        /// Comma-separated object ids to highlight
        #[arg(long, value_delimiter = ',')]
        highlight: Vec<String>,
        /// Full view.screenshot arguments as JSON; flags override it
        #[arg(long)]
        args: Option<String>,
    },
    /// Run a JSON array of {op, args} as one all-or-nothing batch
    Apply {
        batch: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read one JSON request per stdin line against a single in-memory session
    Session {
        /// Write the file after every change
        #[arg(long)]
        autosave: bool,
        /// Refuse mutating and destructive ops
        #[arg(long)]
        read_only: bool,
    },
}

#[derive(Parser)]
#[command(name = "level-cli")]
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
    /// Print the JSON schema of an op's arguments
    Schema { op: String },
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
    #[error("{0}")]
    Render(String),
}

impl CliError {
    fn to_command_error(&self) -> CommandError {
        match self {
            CliError::Io { .. } => CommandError::Io {
                message: self.to_string(),
            },
            CliError::File(FileError::Io { .. }) => CommandError::Io {
                message: self.to_string(),
            },
            CliError::File(FileError::Load { .. }) => CommandError::Format {
                message: self.to_string(),
            },
            CliError::Render(message) => CommandError::Render {
                message: message.clone(),
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

const SESSION_ONLY: &[&str] = &["history.", "select.", "file.new", "file.open"];

fn session_only(op: &str) -> bool {
    SESSION_ONLY.iter().any(|p| op.starts_with(p))
}

fn execute(path: &Path, request: Request) -> Result<Response, CliError> {
    if session_only(&request.op) {
        return Ok(Response::error(
            &CommandError::invalid_arg(
                "op",
                format!(
                    "{} keeps state between commands, which one-shot runs do not",
                    request.op
                ),
            )
            .with_hint("use `level-cli <file> session`, or the MCP server"),
        ));
    }
    let doc = level_fmt::load_file(path)?;
    let mut session = Session::with_path(doc, path.to_path_buf());
    let before = session.revision();
    let dry_run = request.dry_run;
    let response = session.execute(request, "cli");
    if response.is_ok() && !dry_run && session.revision() != before {
        level_fmt::save_file(path, session.document())?;
    }
    Ok(response)
}

struct ScreenshotFlags<'a> {
    camera: &'a Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    mode: &'a Option<String>,
    overlay: &'a [String],
    highlight: &'a [String],
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
    Ok(args)
}

fn take_screenshot(file: &Path, out: &Path, args: Value) -> Result<Response, CliError> {
    let doc = level_fmt::load_file(file)?;
    let renderer = level_render::Renderer::new().map_err(|e| CliError::Render(e.to_string()))?;
    match level_render::screenshot(&renderer, &doc, args) {
        Ok(shot) => {
            level_fmt::write_atomic(out, &shot.png)?;
            let mut result = shot.result();
            result["path"] = json!(out.display().to_string());
            Ok(Response::Success(Success {
                ok: true,
                revision: 0,
                result,
                changes: Changes::default(),
                warnings: shot.warnings,
            }))
        }
        Err(CommandError::Render { message }) => Err(CliError::Render(message)),
        Err(e) => Ok(Response::error(&e)),
    }
}

fn run(cli: &Cli) -> Result<Response, CliError> {
    match &cli.command {
        Cmd::New { name, force } => {
            if cli.file.exists() && !force {
                return Err(CliError::Io {
                    path: cli.file.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "file exists (use --force to overwrite)",
                    ),
                });
            }
            let default_name = cli
                .file
                .file_name()
                .map(|n| {
                    n.to_string_lossy()
                        .trim_end_matches(".level.toml")
                        .to_string()
                })
                .unwrap_or_default();
            let doc = Document::new(name.clone().unwrap_or(default_name));
            level_fmt::save_file(&cli.file, &doc)?;
            let mut session = Session::new(doc);
            Ok(session.execute(Request::new("scene.summary", json!({})), "cli"))
        }
        Cmd::Run {
            op,
            args,
            dry_run,
            expect_revision,
        } => {
            let mut request = Request::new(op.clone(), parse_json("--args", args)?);
            request.dry_run = *dry_run;
            request.expect_revision = *expect_revision;
            execute(&cli.file, request)
        }
        Cmd::Screenshot {
            camera,
            out,
            width,
            height,
            mode,
            overlay,
            highlight,
            args,
        } => {
            let args = screenshot_args(
                args,
                ScreenshotFlags {
                    camera,
                    width: *width,
                    height: *height,
                    mode,
                    overlay,
                    highlight,
                },
            )?;
            take_screenshot(&cli.file, out, args)
        }
        Cmd::Session { .. } => unreachable!("handled in main"),
        Cmd::Apply { batch, dry_run } => {
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
            execute(&cli.file, request)
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
        Meta::Schema { op } => match OpInfo::find(&op) {
            Some(info) => {
                let schema = serde_json::to_value((info.args_schema)()).expect("schemas serialize");
                print_value(&schema, true);
                ExitCode::SUCCESS
            }
            None => {
                let e = CommandError::invalid_arg("op", format!("unknown op `{op}`"))
                    .with_hint("run `level-cli ops` for the list");
                print_value(&Response::error(&e).to_value(), false);
                ExitCode::from(1)
            }
        },
    }
}

fn exec_name(exec: Exec) -> &'static str {
    match exec {
        Exec::Document => "document",
        Exec::Session => "session",
        Exec::Client => "client",
    }
}

fn session_reply(host: &mut Host, line: &str) -> Value {
    let mut request = match serde_json::from_str::<Value>(line) {
        Ok(v) => v,
        Err(e) => {
            return Response::error(&CommandError::InvalidArgument {
                field: None,
                message: format!("not valid JSON: {e}"),
                hint: Some("send one {\"op\": ..., \"args\": ...} object per line".into()),
            })
            .to_value();
        }
    };
    let out_path = if request.get("op").and_then(Value::as_str) == Some("view.screenshot") {
        request
            .get_mut("args")
            .and_then(Value::as_object_mut)
            .and_then(|a| a.remove("path"))
            .and_then(|p| p.as_str().map(PathBuf::from))
    } else {
        None
    };
    let reply = host.execute_value(request, "cli");
    let mut value = reply.response.to_value();
    if let Some(png) = reply.png {
        match &out_path {
            Some(path) => match level_fmt::write_atomic(path, &png) {
                Ok(()) => value["result"]["path"] = json!(path.display().to_string()),
                Err(e) => {
                    return Response::error(&CommandError::Io {
                        message: e.to_string(),
                    })
                    .to_value();
                }
            },
            None => {
                value["result"]["png_base64"] =
                    json!(base64::engine::general_purpose::STANDARD.encode(png));
            }
        }
    }
    value
}

fn session_mode(file: &Path, autosave: bool, read_only: bool) -> Result<(), CliError> {
    let doc = level_fmt::load_file(file)?;
    let policy = if read_only {
        Policy::read_only()
    } else {
        Policy::allow_all()
    };
    let mut host = Host::new(Session::with_path(doc, file.to_path_buf()), policy);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|source| CliError::Io {
            path: PathBuf::from("<stdin>"),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let value = session_reply(&mut host, &line);
        let io = |source| CliError::Io {
            path: PathBuf::from("<stdout>"),
            source,
        };
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&value).expect("responses serialize")
        )
        .and_then(|()| stdout.flush())
        .map_err(io)?;
        if autosave && host.session().is_dirty() {
            let saved = host
                .session_mut()
                .execute(Request::new("file.save", json!({})), "cli");
            if !saved.is_ok() {
                eprintln!("autosave failed: {}", saved.to_value());
            }
        }
    }
    if host.session().is_dirty() {
        eprintln!("warning: unsaved changes were discarded (use file.save or --autosave)");
    }
    Ok(())
}

fn main() -> ExitCode {
    if matches!(std::env::args().nth(1).as_deref(), Some("ops" | "schema")) {
        return meta(MetaCli::parse());
    }
    let cli = Cli::parse();
    if let Cmd::Session {
        autosave,
        read_only,
    } = &cli.command
    {
        return match session_mode(&cli.file, *autosave, *read_only) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                print_value(
                    &Response::error(&e.to_command_error()).to_value(),
                    cli.pretty,
                );
                ExitCode::from(2)
            }
        };
    }
    let (response, code) = match run(&cli) {
        Ok(response) => {
            let code = if response.is_ok() { 0 } else { 1 };
            (response, code)
        }
        Err(e) => (Response::error(&e.to_command_error()), 2),
    };
    let value = response.to_value();
    let text = if cli.pretty {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    }
    .expect("responses serialize");
    println!("{text}");
    ExitCode::from(code)
}
