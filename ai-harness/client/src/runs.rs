//! App-level store for conversations with a turn currently streaming.
//!
//! Owned by [`crate::app::App`] and handed down as context — not owned by
//! [`crate::views::Conversation`] — which is the whole point: that view is
//! remounted (and its own signals disposed) every time the route switches to
//! a different conversation id, but this store isn't, so a turn started
//! before a navigation keeps streaming into this store's state regardless of
//! what the main content area is currently showing. Coming back to that
//! conversation re-reads the same state rather than starting blank.
//!
//! The server side of this is `lib::service::runs` — its module doc explains
//! why this can only ever be a *view* of what the backend already knows, not
//! a second source of truth: a partial response never touches the database,
//! so a client that never attaches (or one that crashed and restarted)
//! simply never sees it, and that's fine.

use std::collections::HashMap;

use leptos::prelude::*;
use shared::conversation::RunStatus;
use shared::ids::ConversationId;
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::transcript::Draft;

/// Where a conversation's run currently stands, from this client's point of
/// view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Nothing running — the persisted view is current.
    #[default]
    Idle,
    /// A `start_message`/`attach_conversation` round trip is in flight, but
    /// no event has arrived yet.
    Attaching,
    /// Actively receiving events.
    Streaming,
}

#[derive(Debug, Clone, Default)]
pub struct RunState {
    pub phase: Phase,
    /// The turn's events, folded into bubbles exactly as
    /// `views::Conversation` folded them before this store existed — see
    /// `transcript::Draft`. Left in place (not cleared) once a turn
    /// finishes, so the view can keep showing it until its own refetch
    /// lands; see [`Runs::forget`].
    pub draft: Draft,
    /// The last `ConversationBusy` (or other) rejection from `send`, or a
    /// run's own `RunStatus::Failed`. Cleared on the next `send`.
    pub error: Option<String>,
}

/// The store: one [`RunState`] signal per conversation that has ever been
/// sent to or attached in this session. Cheap to copy — it's two signal
/// handles — so it's handed around as ordinary `Copy` context, the same way
/// `Route`/the reload counter already are.
#[derive(Debug, Clone, Copy)]
pub struct Runs {
    map: RwSignal<HashMap<i64, RwSignal<RunState>>>,
    /// `App`'s existing reload counter, bumped here too whenever a run
    /// settles — so the sidebar's "awaiting approval" badge and every list
    /// view refetch the same way they already do after a create/send/delete.
    reload: RwSignal<u32>,
}

impl Runs {
    pub fn new(reload: RwSignal<u32>) -> Self {
        Self {
            map: RwSignal::new(HashMap::new()),
            reload,
        }
    }

    /// The signal for `id`, created on first use. Once created, `state`
    /// always returns the *same* signal for a given id for the rest of the
    /// session — a mounted `Conversation` view reads this once and closes
    /// over it for its whole lifetime, so nothing here may ever hand back a
    /// different instance for an id already in use.
    pub fn state(&self, id: ConversationId) -> RwSignal<RunState> {
        if let Some(existing) = self.map.with_untracked(|m| m.get(&id.get()).copied()) {
            return existing;
        }
        let created = RwSignal::new(RunState::default());
        self.map.update(|m| {
            m.entry(id.get()).or_insert(created);
        });
        self.map
            .with_untracked(|m| *m.get(&id.get()).expect("just inserted above"))
    }

    /// Start a message and follow it. A `ConversationBusy` rejection (or any
    /// other failure to even start) lands in this conversation's `error`
    /// rather than being thrown away — the "starting a message on a
    /// conversation the backend thinks is already in flight is an error"
    /// requirement.
    pub fn send(&self, id: ConversationId, text: String) {
        let state = self.state(id);
        state.update(|s| {
            s.phase = Phase::Attaching;
            s.draft = Draft::new();
            s.error = None;
        });
        let this = *self;
        spawn_local(async move {
            match commands::start_message(id, text).await {
                Ok(()) => this.follow(id),
                Err(err) => state.update(|s| {
                    s.phase = Phase::Idle;
                    s.error = Some(err.message);
                }),
            }
        });
    }

    /// Attach if — and only if — nothing is already following this
    /// conversation. Call this on every mount of the conversation view: a
    /// fresh mount for a conversation nobody is watching starts following it
    /// (what makes leaving and coming back resumable); a fresh mount for one
    /// already being followed is a no-op, so there is never more than one
    /// follower per conversation on this client.
    pub fn ensure_following(&self, id: ConversationId) {
        if self.state(id).get_untracked().phase != Phase::Idle {
            return;
        }
        self.follow(id);
    }

    fn follow(&self, id: ConversationId) {
        let state = self.state(id);
        state.update(|s| s.phase = Phase::Attaching);
        let reload = self.reload;
        spawn_local(async move {
            let result = commands::attach_conversation(id, move |event| {
                state.update(|s| {
                    s.phase = Phase::Streaming;
                    s.draft.apply(&event);
                });
            })
            .await;
            match result {
                Ok(RunStatus::Idle) => state.update(|s| s.phase = Phase::Idle),
                Ok(RunStatus::Finished { .. }) => {
                    state.update(|s| s.phase = Phase::Idle);
                    reload.update(|n| *n += 1);
                }
                Ok(RunStatus::Failed { error }) => {
                    state.update(|s| {
                        s.phase = Phase::Idle;
                        s.error = Some(error.message);
                    });
                    reload.update(|n| *n += 1);
                }
                Err(err) => state.update(|s| {
                    s.phase = Phase::Idle;
                    s.error = Some(err.message);
                }),
            }
        });
    }

    /// Drop this conversation's finished draft once it is idle with no error
    /// to show — call this right after a fresh `get_conversation` lands, so
    /// the persisted messages it just fetched don't sit duplicated
    /// indefinitely in a draft nothing renders anymore. The signal itself
    /// (and its slot in `map`) stays alive for the rest of the session —
    /// see `state`'s doc on why an entry can never be removed once created.
    pub fn forget(&self, id: ConversationId) {
        let state = self.state(id);
        let should_clear = {
            let current = state.get_untracked();
            current.phase == Phase::Idle && current.error.is_none() && !current.draft.is_empty()
        };
        if should_clear {
            state.update(|s| s.draft = Draft::new());
        }
    }
}
