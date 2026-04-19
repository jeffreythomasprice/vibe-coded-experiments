pub mod app;
pub mod commands;
pub mod error;
pub mod screens;
pub mod ui;

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};

use crossterm::event::{Event, EventStream};
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
pub use self::error::TuiError;
use self::screens::{Screen, Transition};

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
    Reply {
        seq: u64,
        result: Result<Response, ClientError>,
    },
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
            Some(AppEvent::Reply { seq, result }) = rx.recv() => handle_reply(&mut app, seq, result),
        }
    }
    Ok(())
}

fn handle_term(app: &mut App, event: Event, ctx: &Ctx) {
    let Event::Key(key) = event else {
        return;
    };

    // Dispatch the key through the current screen. The screen returns a
    // Transition which we apply once its borrow on `app.screen` ends.
    let mut to_send: Option<Request> = None;
    let mut tag_action: Option<screens::tag_editor::TagAction> = None;
    let mut search_should_search = false;
    let mut confirm_action: Option<screens::confirm::ConfirmAction> = None;
    let transition = match &mut app.screen {
        Screen::Chat(state) => screens::chat::handle_key(state, key, |req| {
            to_send = Some(req);
        }),
        Screen::ConversationList(state) => screens::conversation_list::handle_key(state, key),
        Screen::TagEditor(state) => {
            tag_action = screens::tag_editor::handle_key(state, key);
            Transition::None
        }
        Screen::Search(state) => match screens::search::handle_key(state, key) {
            screens::search::SearchAction::None => Transition::None,
            screens::search::SearchAction::Search => {
                search_should_search = true;
                Transition::None
            }
            screens::search::SearchAction::Transition(t) => t,
        },
        Screen::ConfirmDelete(state) => {
            confirm_action = Some(screens::confirm::handle_key(state, key));
            Transition::None
        }
    };

    if let Some(req) = to_send {
        // Requests raised by the current screen's handle_key are tagged with
        // the current screen's main request_seq so replies route back
        // correctly (only used by Chat for its normal turn flow).
        let seq = spawn_request(app, ctx, req);
        if let Screen::Chat(state) = &mut app.screen {
            state.request_seq = seq;
        }
    }

    if let Some(action) = tag_action {
        apply_tag_action(app, ctx, action);
    }

    if search_should_search && let Screen::Search(state) = &mut app.screen {
        state.loading = true;
        state.results.clear();
        let req = screens::search::search_request(state);
        let seq = spawn_request(app, ctx, req);
        if let Screen::Search(state) = &mut app.screen {
            state.results_seq = seq;
        }
    }

    if let Some(action) = confirm_action {
        apply_confirm_action(app, ctx, action);
    }

    apply_transition(app, ctx, transition);
}

fn apply_confirm_action(app: &mut App, ctx: &Ctx, action: screens::confirm::ConfirmAction) {
    use screens::confirm::ConfirmAction;
    match action {
        ConfirmAction::None => {}
        ConfirmAction::Back => {
            let back = match &app.screen {
                Screen::ConfirmDelete(state) => screens::confirm::back_transition(state),
                _ => Transition::None,
            };
            apply_transition(app, ctx, back);
        }
        ConfirmAction::Confirm => {
            let (conv_id, back) = match &app.screen {
                Screen::ConfirmDelete(state) => (
                    state.conv_id.clone(),
                    screens::confirm::back_transition(state),
                ),
                _ => return,
            };
            spawn_request(app, ctx, Request::ConversationDelete { id: conv_id });
            apply_transition(app, ctx, back);
        }
    }
}

fn apply_transition(app: &mut App, ctx: &Ctx, t: Transition) {
    match t {
        Transition::None => {}
        Transition::Quit => app.should_quit = true,
        Transition::To(screen) => {
            app.screen = screen;
            // Fire any initial loads the new screen wants, and stash the
            // seqs back onto it so the replies can be routed.
            let reqs = app.screen.on_enter();
            let seqs: Vec<u64> = reqs
                .into_iter()
                .map(|r| spawn_request(app, ctx, r))
                .collect();
            app.screen.record_seqs(&seqs);
        }
    }
}

fn apply_tag_action(app: &mut App, ctx: &Ctx, action: screens::tag_editor::TagAction) {
    use screens::tag_editor::TagAction;
    let (req, optimistic_add, optimistic_remove) = match action {
        TagAction::AddTag(tag) => {
            let conv_id = match &app.screen {
                Screen::TagEditor(state) => state.conversation_id.clone(),
                _ => return,
            };
            (
                Request::ConversationAddTag {
                    id: conv_id,
                    tag: tag.clone(),
                },
                Some(tag),
                None,
            )
        }
        TagAction::RemoveTag(tag) => {
            let conv_id = match &app.screen {
                Screen::TagEditor(state) => state.conversation_id.clone(),
                _ => return,
            };
            (
                Request::ConversationRemoveTag {
                    id: conv_id,
                    tag: tag.clone(),
                },
                None,
                Some(tag),
            )
        }
        TagAction::Back => {
            let back = match &app.screen {
                Screen::TagEditor(state) => screens::tag_editor::back_transition(state),
                _ => Transition::None,
            };
            apply_transition(app, ctx, back);
            return;
        }
    };

    // Optimistic UI update: apply the change locally, THEN send the RPC.
    // The server's ack doesn't need to be routed back into the tag editor's
    // load-seqs; its reply is handled as a no-op (see tag_editor::handle_reply).
    if let Screen::TagEditor(state) = &mut app.screen {
        if let Some(tag) = optimistic_add
            && !state.tags.contains(&tag)
        {
            state.tags.push(tag);
            state.tags.sort();
        }
        if let Some(tag) = optimistic_remove {
            state.tags.retain(|t| t != &tag);
            if state.selected_tag >= state.tags.len() && !state.tags.is_empty() {
                state.selected_tag = state.tags.len() - 1;
            }
        }
    }
    spawn_request(app, ctx, req);
}

fn spawn_request(app: &mut App, ctx: &Ctx, req: Request) -> u64 {
    let seq = app.next_request_seq;
    app.next_request_seq += 1;
    app.pending += 1;
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
        let _ = tx.send(AppEvent::Reply { seq, result });
    });
    seq
}

fn handle_reply(app: &mut App, seq: u64, result: Result<Response, ClientError>) {
    if app.pending > 0 {
        app.pending -= 1;
    }

    // Route based on the current screen. Each screen decides whether the
    // reply is one of theirs (matches an outstanding seq) or stale.
    match &mut app.screen {
        Screen::Chat(state) => {
            // Chat accepts normal-turn replies (no outstanding request_seq =
            // 0) as well as its own ConversationGet reply when request_seq
            // matches.
            if state.request_seq == seq || state.request_seq == 0 {
                state.request_seq = 0;
                match result {
                    Ok(resp) => state.display_response(resp),
                    Err(err) => state.push_error(format!("request failed: {err}")),
                }
            }
        }
        Screen::ConversationList(state) => {
            if state.request_seq == seq {
                match result {
                    Ok(resp) => screens::conversation_list::handle_reply(state, resp),
                    Err(_) => state.loading = false,
                }
            }
        }
        Screen::TagEditor(state) => {
            screens::tag_editor::handle_reply(state, seq, result);
        }
        Screen::Search(state) => {
            screens::search::handle_reply(state, seq, result);
        }
        Screen::ConfirmDelete(_) => {
            // ConfirmDelete has no outstanding requests of its own; any
            // delete reply arrives after the transition has already happened.
        }
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
