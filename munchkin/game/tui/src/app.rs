//! The tui's application state and the event loop that drives it.
//!
//! [`App`] holds everything the UI renders; [`run_ui`] is the loop: it draws a
//! frame, then `select!`s over a keepalive ping interval, a one-second redraw
//! tick (so the "last ping Xs ago" text stays live), and the terminal's async
//! key-event stream. Pressing Esc breaks the loop and returns.
//!
//! Keepalive requests reuse the same one-in-flight [`IpcClient`] the handshake
//! used. A failed request does not tear the UI down: we flag the connection as
//! lost, surface the message, and keep showing the (now stale) status until the
//! user quits.

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures_util::StreamExt;
use ratatui::DefaultTerminal;

use shared::client::IpcClient;
use shared::protocol::{ClientInfo, Request, Response};

use crate::session::ServerIdentity;
use crate::ui;

/// How often to ping the engine to stay alive.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Ask for stats every Nth ping.
const STATS_EVERY: u64 = 4;

/// How often to redraw so elapsed-time text refreshes.
const REDRAW_INTERVAL: Duration = Duration::from_secs(1);

/// Everything the UI needs to render.
pub struct App {
    /// Whether we still believe the engine connection is live.
    pub connected: bool,
    /// Who we handshook with.
    pub server: ServerIdentity,
    /// When we last got a `Pong`, for the "Xs ago" display. Monotonic, so it's
    /// immune to wall-clock skew.
    pub last_ping: Option<Instant>,
    /// The most recent `Stats` snapshot of connected clients.
    pub clients: Vec<ClientInfo>,
    /// The last error that knocked us offline, if any.
    pub error: Option<String>,
}

impl App {
    /// Create the initial state for a freshly-handshook connection.
    pub fn new(server: ServerIdentity) -> Self {
        Self {
            connected: true,
            server,
            last_ping: None,
            clients: Vec::new(),
            error: None,
        }
    }

    fn note_ping(&mut self) {
        self.last_ping = Some(Instant::now());
    }

    fn set_clients(&mut self, clients: Vec<ClientInfo>) {
        self.clients = clients;
    }

    fn mark_disconnected(&mut self, err: anyhow::Error) {
        tracing::warn!(%err, "engine connection lost");
        self.connected = false;
        self.error = Some(err.to_string());
    }
}

/// Draw the UI and pump events until the user presses Esc (or stdin closes).
///
/// Returns `Ok(())` on a clean quit. A lost engine connection is *not* an error
/// here — it updates [`App`] and the loop keeps running so the status stays
/// visible until the user quits.
pub async fn run_ui(
    terminal: &mut DefaultTerminal,
    client: &mut IpcClient,
    app: &mut App,
) -> Result<()> {
    let mut ping = tokio::time::interval(PING_INTERVAL);
    // A slow request shouldn't queue up a burst of catch-up pings.
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut redraw = tokio::time::interval(REDRAW_INTERVAL);
    let mut events = EventStream::new();
    let mut n: u64 = 0;

    loop {
        // Draw before blocking so the first frame shows immediately and every
        // wake re-renders (this is what keeps "Xs ago" ticking).
        terminal.draw(|frame| ui::draw(frame, app))?;

        tokio::select! {
            _ = ping.tick() => {
                if app.connected {
                    n += 1;
                    // Don't `?` on request errors: a dropped connection should
                    // be shown, not crash the UI.
                    match client.request(Request::Ping).await {
                        Ok(Response::Pong) => app.note_ping(),
                        Ok(other) => tracing::warn!(?other, "unexpected ping response"),
                        Err(e) => app.mark_disconnected(e),
                    }

                    // Fetch stats on the very first ping (so the client list —
                    // including ourselves — shows up right away) and every Nth
                    // ping thereafter.
                    if app.connected && (n == 1 || n % STATS_EVERY == 0) {
                        match client.request(Request::Stats).await {
                            Ok(Response::Stats { clients }) => app.set_clients(clients),
                            Ok(other) => tracing::warn!(?other, "unexpected stats response"),
                            Err(e) => app.mark_disconnected(e),
                        }
                    }
                }
            }
            // No-op: the redraw happens at the top of the loop.
            _ = redraw.tick() => {}
            maybe = events.next() => {
                match maybe {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        if key.code == KeyCode::Esc {
                            break;
                        }
                    }
                    // Resize / other events: the loop-top draw handles it.
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::error!(%e, "terminal event stream error");
                        break;
                    }
                    // stdin closed under us.
                    None => break,
                }
            }
        }
    }

    Ok(())
}
