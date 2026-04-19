pub mod app;
pub mod commands;
pub mod error;
pub mod ui;

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::client;
use crate::config::Config;
use crate::error::ClientError;
use crate::protocol::{Request, Response};

use self::app::App;
use self::commands::{Action, SlashCommand};
pub use self::error::TuiError;

pub async fn run(
    cfg: &Config,
    socket: &Path,
    config_override: Option<&Path>,
    secrets_override: Option<&Path>,
) -> Result<(), TuiError> {
    install_panic_hook();
    let mut term = setup_terminal()?;
    let result = run_loop(
        &mut term,
        cfg.clone(),
        socket.to_path_buf(),
        config_override.map(Path::to_path_buf),
        secrets_override.map(Path::to_path_buf),
    )
    .await;
    let _ = restore_terminal(&mut term);
    result
}

enum AppEvent {
    Reply(Result<Response, ClientError>),
}

/// Owned context shared with spawned request tasks.
struct Ctx {
    cfg: Config,
    socket: PathBuf,
    cfg_override: Option<PathBuf>,
    sec_override: Option<PathBuf>,
    tx: UnboundedSender<AppEvent>,
}

async fn run_loop(
    term: &mut Terminal<CrosstermBackend<Stdout>>,
    cfg: Config,
    socket: PathBuf,
    cfg_override: Option<PathBuf>,
    sec_override: Option<PathBuf>,
) -> Result<(), TuiError> {
    let (tx, mut rx): (UnboundedSender<AppEvent>, UnboundedReceiver<AppEvent>) =
        tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        cfg,
        socket,
        cfg_override,
        sec_override,
        tx,
    };
    let mut term_stream = EventStream::new();
    let mut app = App::new();

    loop {
        term.draw(|f| ui::render(f, &app))?;
        if app.should_quit {
            break;
        }
        tokio::select! {
            ev = term_stream.next() => match ev {
                Some(Ok(event)) => handle_term(&mut app, event, &ctx),
                Some(Err(err)) => return Err(TuiError::Io(err)),
                None => break,
            },
            Some(AppEvent::Reply(result)) = rx.recv() => handle_reply(&mut app, result),
        }
    }
    Ok(())
}

fn handle_term(app: &mut App, event: Event, ctx: &Ctx) {
    let Event::Key(key) = event else {
        return;
    };
    if key.kind == KeyEventKind::Release {
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }
    match key.code {
        KeyCode::Esc => {
            if app.autocomplete.is_some() {
                app.autocomplete = None;
            } else {
                app.should_quit = true;
            }
        }
        KeyCode::Tab => app.cycle_autocomplete(true),
        KeyCode::BackTab => app.cycle_autocomplete(false),
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Left => app.cursor_left(),
        KeyCode::Right => app.cursor_right(),
        KeyCode::Up => app.scroll_offset = app.scroll_offset.saturating_sub(1),
        KeyCode::Down => app.scroll_offset = app.scroll_offset.saturating_add(1),
        KeyCode::Enter => submit(app, ctx),
        KeyCode::Char(c) => app.input_insert(c),
        _ => {}
    }
}

fn submit(app: &mut App, ctx: &Ctx) {
    let input = app.take_input();
    if input.is_empty() {
        return;
    }
    app.push_user(input.clone());

    if let Some(rest) = input.strip_prefix('/') {
        let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if name.is_empty() {
            app.push_error("empty command");
            return;
        }
        let cmd = commands::find(name).or_else(|| commands::filter(name).into_iter().next());
        match cmd {
            Some(cmd) => run_action(app, cmd, args, ctx),
            None => app.push_error(format!("unknown command: /{name}")),
        }
    } else {
        spawn_request(ctx, Request::Chat { message: input });
        app.pending += 1;
    }
}

fn run_action(app: &mut App, cmd: &SlashCommand, args: &str, ctx: &Ctx) {
    match cmd.action {
        Action::Quit => app.should_quit = true,
        Action::Send(builder) => {
            spawn_request(ctx, builder(args));
            app.pending += 1;
        }
    }
}

fn spawn_request(ctx: &Ctx, req: Request) {
    let cfg = ctx.cfg.clone();
    let socket = ctx.socket.clone();
    let cfg_override = ctx.cfg_override.clone();
    let sec_override = ctx.sec_override.clone();
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let result = client::round_trip(
            &cfg,
            &socket,
            cfg_override.as_deref(),
            sec_override.as_deref(),
            req,
        )
        .await;
        let _ = tx.send(AppEvent::Reply(result));
    });
}

fn handle_reply(app: &mut App, result: Result<Response, ClientError>) {
    if app.pending > 0 {
        app.pending -= 1;
    }
    match result {
        Ok(resp) => app.display_response(resp),
        Err(err) => app.push_error(format!("request failed: {err}")),
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(term: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    term.show_cursor()?;
    Ok(())
}

fn install_panic_hook() {
    let prior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        prior(info);
    }));
}
