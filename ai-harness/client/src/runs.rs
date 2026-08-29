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
use shared::agent::ToolDecision;
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
    /// A mount's one-shot `ensure_following` attach is in flight. Distinct
    /// from `Attaching` so the UI doesn't flash a "Responding…" spinner and a
    /// disabled composer every time an already-settled conversation is
    /// opened — this is just the round trip that finds out nothing is
    /// running, not a run the client itself started.
    Probing,
    /// A `start_message`/`attach_conversation` round trip started by
    /// [`Runs::send`] is in flight, but no event has arrived yet.
    Attaching,
    /// Actively receiving events.
    Streaming,
}

impl Phase {
    /// Whether the UI should show this conversation as running: a spinner
    /// and a disabled composer. `Probing` is deliberately excluded — see its
    /// doc.
    pub fn is_busy(self) -> bool {
        matches!(self, Phase::Attaching | Phase::Streaming)
    }
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
    /// `ArcRwSignal`, not `RwSignal`: an `RwSignal` is an arena item,
    /// registered with whatever `Owner` is current when it's constructed and
    /// dropped when that owner is cleaned up. `state`'s first call for a
    /// conversation runs inside `views::Conversation`'s own owner, which is
    /// exactly the owner a navigation away disposes — a stored `RwSignal`
    /// would go dead in the map right then, with every future `.get()`/
    /// `.update()` from `follow`'s still-running task either silently
    /// dropped or (on the read side) panicking the whole wasm module. An
    /// `ArcRwSignal` is reference-counted, not owner-scoped, so it can't be
    /// disposed out from under this map no matter which owner first asked
    /// for it.
    map: RwSignal<HashMap<i64, ArcRwSignal<RunState>>>,
    /// `App`'s existing reload counter, bumped here too whenever a run
    /// settles — so the sidebar's "awaiting approval" badge and every list
    /// view refetch the same way they already do after a create/send/delete.
    reload: RwSignal<u32>,
}

impl Runs {
    pub fn new(reload: RwSignal<u32>) -> Self {
        Self { map: RwSignal::new(HashMap::new()), reload }
    }

    /// The signal for `id`, created on first use. Once created, `state`
    /// always returns a handle to the *same* signal for a given id for the
    /// rest of the session — a mounted `Conversation` view reads this once
    /// and closes over it for its whole lifetime, so nothing here may ever
    /// hand back a different instance for an id already in use. See `map`'s
    /// doc on why that promise survives the calling owner being disposed.
    pub fn state(&self, id: ConversationId) -> ArcRwSignal<RunState> {
        if let Some(existing) = self.map.with_untracked(|m| m.get(&id.get()).cloned()) {
            return existing;
        }
        let created = ArcRwSignal::new(RunState::default());
        self.map.update(|m| {
            m.entry(id.get()).or_insert_with(|| created.clone());
        });
        self.map.with_untracked(|m| m.get(&id.get()).expect("just inserted above").clone())
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
                Ok(()) => this.follow(id, Phase::Attaching),
                Err(err) => state.update(|s| {
                    s.phase = Phase::Idle;
                    s.error = Some(err.message);
                }),
            }
        });
    }

    /// The `approve_tools` counterpart to [`Runs::send`] — resolves this
    /// conversation's pending tool calls and follows the turn forward from
    /// there. `start_approve_tools` is the detached counterpart of
    /// `start_message` (see its own doc), so it registers a run the same
    /// way and this reuses the identical `follow` machinery.
    pub fn approve(&self, id: ConversationId, decisions: Vec<ToolDecision>) {
        let state = self.state(id);
        state.update(|s| {
            s.phase = Phase::Attaching;
            s.draft = Draft::new();
            s.error = None;
        });
        let this = *self;
        spawn_local(async move {
            match commands::start_approve_tools(id, decisions).await {
                Ok(()) => this.follow(id, Phase::Attaching),
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
    /// follower per conversation on this client. Enters `Phase::Probing`
    /// rather than `Phase::Attaching` — this call doesn't know yet whether
    /// anything is actually running, and most of the time nothing is, so it
    /// shouldn't flash the "busy" UI for the one round trip it takes to find
    /// that out.
    pub fn ensure_following(&self, id: ConversationId) {
        if self.state(id).get_untracked().phase != Phase::Idle {
            return;
        }
        self.follow(id, Phase::Probing);
    }

    fn follow(&self, id: ConversationId, entering: Phase) {
        let state = self.state(id);
        state.update(|s| s.phase = entering);
        let reload = self.reload;
        spawn_local(async move {
            // Cloned, not moved: `state` is `ArcRwSignal`, not `Copy`, and is
            // still needed below once this per-event closure has been handed
            // off to `attach_conversation` for the whole call.
            let sink_state = state.clone();
            let result = commands::attach_conversation(id, move |event| {
                sink_state.update(|s| {
                    s.phase = Phase::Streaming;
                    s.draft.apply(&event);
                });
            })
            .await;
            // Every branch that reflects a real settle — including `Idle`,
            // which means a turn finished (or was never there) — bumps
            // `reload`. Without this, a turn that settles in the gap between
            // `start_message` returning and `attach_conversation` starting
            // would report `Idle` here and nothing would ever refetch it.
            match result {
                Ok(RunStatus::Idle) => {
                    state.update(|s| s.phase = Phase::Idle);
                    reload.update(|n| *n += 1);
                }
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
                // Nothing went wrong — this conversation (or the app) was
                // deleted out from under the run. No error to show; the
                // reload is what makes a still-open sidebar/list stop
                // showing it.
                Ok(RunStatus::Cancelled) => {
                    state.update(|s| s.phase = Phase::Idle);
                    reload.update(|n| *n += 1);
                }
                Err(err) => state.update(|s| {
                    s.phase = Phase::Idle;
                    s.error = Some(err.message);
                }),
            }
        });
    }

    /// Drop this conversation's finished draft once nothing is actively
    /// streaming into it and there's no error to show — call this right
    /// after a fresh `get_conversation` lands, so the persisted messages it
    /// just fetched don't sit duplicated indefinitely in a draft nothing
    /// renders anymore. `Streaming` is the only phase that appends to the
    /// draft, so any other phase is safe to clear: `send` clears it itself
    /// on the way into `Attaching`, and a `Probing` attach that does find a
    /// live run replays the whole backlog, so nothing is lost by clearing
    /// beforehand. `error` stays excluded: a failed turn's partial output
    /// never reaches the database (see `lib::db`'s module doc), so the draft
    /// is the only surviving copy of it until the user sends again. The
    /// signal itself (and its slot in `map`) stays alive for the rest of the
    /// session — see `state`'s doc on why an entry can never be removed once
    /// created.
    pub fn forget(&self, id: ConversationId) {
        let state = self.state(id);
        let should_clear = {
            let current = state.get_untracked();
            current.phase != Phase::Streaming && current.error.is_none() && !current.draft.is_empty()
        };
        if should_clear {
            state.update(|s| s.draft = Draft::new());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root owner, `set` as current — stands in for the one `App` installs
    /// for real (`crate::app::App` never constructs one explicitly; Leptos's
    /// own mount point does). Everything created after this call while no
    /// more specific owner is current — including a bare `Runs::new` — is
    /// registered under it.
    fn root_owner() -> Owner {
        let owner = Owner::new();
        owner.set();
        owner
    }

    /// An event that actually pushes a bubble into a [`Draft`] — unlike
    /// `StepStart`, which only resets its open-block tracking (see
    /// `Draft::apply`) — so a test can put a draft into a non-empty state.
    fn tool_start_event() -> shared::agent::AgentEvent {
        shared::agent::AgentEvent::ToolStart {
            tool_use_id: "call_0_0".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({}),
        }
    }

    #[test]
    fn state_returns_the_same_signal_for_an_id() {
        let _root = root_owner();
        let runs = Runs::new(RwSignal::new(0));
        let id = ConversationId(7);
        assert_eq!(runs.state(id), runs.state(id));
    }

    /// The regression test for the bug this module's doc warns about: a
    /// mounted `Conversation` view is a child owner asking for its
    /// conversation's state for the first time; navigating away disposes
    /// that owner. `state` must keep handing back the *same live* signal
    /// afterward, not the disposed one — reads must not panic, and a write
    /// through a freshly re-fetched handle must be visible through the
    /// original one too, since they are the same signal.
    #[test]
    fn state_survives_the_disposal_of_the_owner_that_first_asked_for_it() {
        let _root = root_owner();
        let runs = Runs::new(RwSignal::new(0));
        let id = ConversationId(1);

        let view_owner = Owner::new();
        let first_mount = view_owner.with(|| runs.state(id));
        assert_eq!(first_mount.get_untracked().phase, Phase::Idle);

        // Navigating away from the conversation.
        view_owner.cleanup();

        // Coming back: a new mount asks `Runs` for this id's state again.
        let second_mount = runs.state(id);
        second_mount.update(|s| s.phase = Phase::Streaming);

        assert_eq!(first_mount.get_untracked().phase, Phase::Streaming);
    }

    /// Strictly stronger than the test above: it disposes *every* owner,
    /// including the root `Runs::new` itself ran under — something the
    /// previous owner-captured-`RwSignal` design could never survive, since
    /// every `RwSignal` it created (root included) would go down with it.
    /// A handle obtained before disposal must still read back its
    /// last-written value, because `ArcRwSignal` is reference-counted, not
    /// registered with any owner's arena — no `cleanup()` call, however
    /// total, can take it down.
    #[test]
    fn state_survives_disposal_of_every_owner_including_root() {
        let root = root_owner();
        let runs = Runs::new(RwSignal::new(0));
        let id = ConversationId(1);

        let view_owner = Owner::new();
        let handle = view_owner.with(|| runs.state(id));
        handle.update(|s| s.phase = Phase::Streaming);

        view_owner.cleanup();
        root.cleanup();

        assert_eq!(handle.get_untracked().phase, Phase::Streaming);
    }

    #[test]
    fn phase_is_busy_only_while_attaching_or_streaming() {
        assert!(!Phase::Idle.is_busy());
        assert!(!Phase::Probing.is_busy());
        assert!(Phase::Attaching.is_busy());
        assert!(Phase::Streaming.is_busy());
    }

    #[test]
    fn forget_clears_a_settled_draft_with_no_error() {
        let _root = root_owner();
        let runs = Runs::new(RwSignal::new(0));
        let id = ConversationId(1);
        let state = runs.state(id);
        state.update(|s| {
            s.phase = Phase::Idle;
            s.draft.apply(&tool_start_event());
        });

        runs.forget(id);

        assert!(state.get_untracked().draft.is_empty());
    }

    #[test]
    fn forget_leaves_a_streaming_draft_alone() {
        let _root = root_owner();
        let runs = Runs::new(RwSignal::new(0));
        let id = ConversationId(1);
        let state = runs.state(id);
        state.update(|s| {
            s.phase = Phase::Streaming;
            s.draft.apply(&tool_start_event());
        });

        runs.forget(id);

        assert!(!state.get_untracked().draft.is_empty());
    }

    #[test]
    fn forget_leaves_a_failed_draft_alone_so_its_only_copy_survives() {
        let _root = root_owner();
        let runs = Runs::new(RwSignal::new(0));
        let id = ConversationId(1);
        let state = runs.state(id);
        state.update(|s| {
            s.phase = Phase::Idle;
            s.error = Some("boom".to_string());
            s.draft.apply(&tool_start_event());
        });

        runs.forget(id);

        assert!(!state.get_untracked().draft.is_empty());
    }
}
