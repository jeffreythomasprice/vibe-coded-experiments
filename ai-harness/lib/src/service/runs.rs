//! In-flight turn tracking: lets a client detach from a streaming
//! conversation and reattach later — switching to another conversation and
//! back, say — without losing the tokens that arrived while it was gone.
//!
//! Process memory only, deliberately. `lib::db`'s module doc explains why
//! nothing partial ever reaches `messages`: a truncated assistant message
//! can end mid-`ToolUse`, or carry a `Thinking` block whose signature never
//! arrived, and persisting it would poison the next send. This is the
//! volatile counterpart that makes a partial response *visible* anyway,
//! without ever risking landing it in the database — on restart there is
//! nothing here to reap, consistent with `db::reap_orphaned`'s invariant
//! that every `running` row is dead by construction at startup.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use shared::agent::AgentEvent;
use shared::conversation::RunStatus;
use shared::ids::ConversationId;

use super::{EventSink, ServiceError};

/// One turn's live event history and its followers.
#[derive(Debug)]
pub(crate) struct Run {
    /// Carried only for `tracing` context — every log line below names the
    /// conversation a subscriber attach/detach or a settle belongs to, since
    /// none of that is otherwise visible from outside this process (see this
    /// module's doc).
    conversation_id: ConversationId,
    state: Mutex<RunState>,
}

#[derive(Debug)]
struct RunState {
    /// Every event this turn has emitted, in order. Bounded by one turn's
    /// `max_tokens × max_steps` and dropped with the `Run` when its owning
    /// `Runs` entry is removed; never truncated, since a partial buffer
    /// would replay as a corrupt transcript.
    events: Vec<AgentEvent>,
    /// Keyed by a monotonic slot rather than held in a bare `Vec`, so a
    /// follower can deregister itself precisely — see [`Subscription`].
    subscribers: HashMap<u64, mpsc::UnboundedSender<AgentEvent>>,
    next_slot: u64,
    /// Set once the turn settles. A follower reads this off the return of
    /// its own `recv` loop, after its channel closes — see
    /// [`Run::subscribe`] and [`Run::finish`].
    finished: Option<RunStatus>,
}

impl Run {
    fn new(conversation_id: ConversationId) -> Arc<Self> {
        Arc::new(Self {
            conversation_id,
            state: Mutex::new(RunState {
                events: Vec::new(),
                subscribers: HashMap::new(),
                next_slot: 0,
                finished: None,
            }),
        })
    }

    /// Record `event` and forward it to every live follower. A sender whose
    /// receiver was dropped without going through `Subscription`'s `Drop`
    /// (never happens via this module's own API, but is cheap insurance
    /// against a future caller that doesn't use it) is swept here too — logged
    /// at `trace` since it's the one path here that shouldn't happen, not
    /// because it's expected to be noisy: unlike `publish` itself, this fires
    /// only on that fallback, not once per event.
    fn publish(&self, event: AgentEvent) {
        let mut state = self.state.lock().expect("run state poisoned");
        state.events.push(event.clone());
        let before = state.subscribers.len();
        state.subscribers.retain(|_, tx| tx.send(event.clone()).is_ok());
        let dropped = before - state.subscribers.len();
        if dropped > 0 {
            tracing::trace!(
                conversation_id = self.conversation_id.get(),
                dropped,
                "swept a subscriber whose receiver was dropped without going through Subscription"
            );
        }
    }

    /// Record the turn's terminal status and close every follower's
    /// channel. Each follower's `recv` loop drains whatever was already
    /// queued before its channel reports closed, so every follower sees
    /// every event before it sees the run end.
    pub(crate) fn finish(&self, status: RunStatus) {
        let mut state = self.state.lock().expect("run state poisoned");
        let followers = state.subscribers.len();
        tracing::debug!(
            conversation_id = self.conversation_id.get(),
            followers,
            status = ?status,
            "run settled; closing every follower's channel"
        );
        state.finished = Some(status);
        state.subscribers.clear();
    }

    /// The turn's terminal status, once `finish` has recorded one.
    fn status(&self) -> Option<RunStatus> {
        self.state.lock().expect("run state poisoned").finished.clone()
    }

    /// A snapshot of everything published so far, plus a live subscription
    /// for what comes next — both taken under one lock, so nothing
    /// published between the snapshot and the subscription taking effect is
    /// ever lost or duplicated. Logged at `trace`: this is the "a new
    /// subscriber showed up" event a client's `attach_conversation` produces
    /// on every mount (including a mere probe that finds nothing running),
    /// so it isn't rare, but it's exactly what pins a client-reported "stuck
    /// on load" down to whether a follower ever actually attached.
    fn subscribe(self: &Arc<Self>) -> (Vec<AgentEvent>, Subscription) {
        let mut state = self.state.lock().expect("run state poisoned");
        let events = state.events.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let slot = state.next_slot;
        state.next_slot += 1;
        state.subscribers.insert(slot, tx);
        let subscriber_count = state.subscribers.len();
        tracing::trace!(
            conversation_id = self.conversation_id.get(),
            slot,
            backlog = events.len(),
            subscriber_count,
            "subscriber attached"
        );
        (
            events,
            Subscription {
                run: Arc::clone(self),
                slot,
                rx,
            },
        )
    }
}

/// Lets `Run::publish` double as the [`EventSink`] a driving turn writes
/// into — the same trait `server`'s Tauri channel implements, so
/// `Service::drive` needs no change to feed one.
impl EventSink for Run {
    fn emit(&self, event: &AgentEvent) {
        self.publish(event.clone());
    }
}

/// A live follower's registration on a [`Run`]. Dropping this — the run
/// settling and the caller returning normally, or a dead IPC channel
/// breaking the caller's loop early — deregisters its slot, so a run nobody
/// is watching anymore holds no dangling senders.
pub(crate) struct Subscription {
    run: Arc<Run>,
    slot: u64,
    rx: mpsc::UnboundedReceiver<AgentEvent>,
}

impl Subscription {
    async fn recv(&mut self) -> Option<AgentEvent> {
        self.rx.recv().await
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut state = self.run.state.lock().expect("run state poisoned");
        state.subscribers.remove(&self.slot);
        let subscriber_count = state.subscribers.len();
        tracing::trace!(
            conversation_id = self.run.conversation_id.get(),
            slot = self.slot,
            subscriber_count,
            "subscriber detached"
        );
    }
}

/// The registry of conversations with a turn currently in flight. Lives on
/// [`super::Service`] alongside (not instead of) its existing
/// `try_lock_conversation` map — that lock stays the fine-grained backstop
/// against a bypass of this registry; this registry is what makes a run
/// followable and replayable.
#[derive(Default)]
pub(crate) struct Runs {
    inner: Mutex<HashMap<i64, Arc<Run>>>,
}

impl Runs {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Register a new run for `id`, or fail if one is already registered.
    /// The in-memory, synchronous-to-call counterpart to
    /// `Service::try_lock_conversation`: a caller that reserves here before
    /// spawning a turn's driving task knows immediately, not after the fact,
    /// whether a second concurrent start is being rejected.
    pub(crate) fn begin(&self, id: ConversationId) -> Result<Arc<Run>, ServiceError> {
        let mut inner = self.inner.lock().expect("runs map poisoned");
        if inner.contains_key(&id.get()) {
            tracing::debug!(
                conversation_id = id.get(),
                "refusing to start a run: this conversation already has one registered"
            );
            return Err(ServiceError::ConversationBusy { conversation_id: id.get() });
        }
        let run = Run::new(id);
        inner.insert(id.get(), Arc::clone(&run));
        tracing::debug!(conversation_id = id.get(), "run registered");
        Ok(run)
    }

    pub(crate) fn get(&self, id: ConversationId) -> Option<Arc<Run>> {
        self.inner.lock().expect("runs map poisoned").get(&id.get()).cloned()
    }

    pub(crate) fn end(&self, id: ConversationId) {
        self.inner.lock().expect("runs map poisoned").remove(&id.get());
        tracing::debug!(conversation_id = id.get(), "run deregistered");
    }
}

/// Replay `run`'s history into `emit`, then forward live events until the
/// run settles or `emit` asks to stop.
///
/// Takes a fallible closure rather than an [`EventSink`]: a sink that is
/// *driving* a turn must never abort it on a send failure — the result
/// still has to be persisted, see `EventSink::emit`'s doc — but a sink that
/// is only *following* has nothing to persist, so a dead channel means
/// stop. Returns `None` if `emit` returned `false` before the run reached a
/// terminal status (the caller's channel is gone; there is nothing left to
/// deliver a status to); `Some(status)` once the run actually settles.
pub(crate) async fn follow(run: Arc<Run>, mut emit: impl FnMut(&AgentEvent) -> bool) -> Option<RunStatus> {
    let conversation_id = run.conversation_id.get();
    let (backlog, mut subscription) = run.subscribe();
    for event in &backlog {
        if !emit(event) {
            tracing::debug!(conversation_id, "follower's sink declined during backlog replay; detaching");
            return None;
        }
    }
    while let Some(event) = subscription.recv().await {
        if !emit(&event) {
            tracing::debug!(conversation_id, "follower's sink declined a live event; detaching");
            return None;
        }
    }
    // The channel closing (rather than `emit` declining) means the run
    // settled — drop the subscription first so `run` is the only handle
    // left, then read the status it just recorded.
    drop(subscription);
    let status = run.status();
    tracing::debug!(conversation_id, status = ?status, "follower observed the run settle");
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::llm::message::{StopReason, Usage};

    fn done_status() -> RunStatus {
        RunStatus::Finished {
            outcome: shared::conversation::TurnOutcome::Done {
                turn_id: shared::ids::TurnId(1),
                steps: 1,
                usage: Usage::default(),
                stop_reason: StopReason::EndTurn,
                messages: vec![],
            },
        }
    }

    fn step_start(step: u32) -> AgentEvent {
        AgentEvent::StepStart { step }
    }

    #[tokio::test]
    async fn begin_twice_on_the_same_conversation_is_conversation_busy() {
        let runs = Runs::new();
        let id = ConversationId(1);
        let _run = runs.begin(id).unwrap();
        let err = runs.begin(id).unwrap_err();
        assert!(matches!(err, ServiceError::ConversationBusy { conversation_id } if conversation_id == 1));
    }

    #[tokio::test]
    async fn end_frees_the_conversation_to_begin_again() {
        let runs = Runs::new();
        let id = ConversationId(1);
        let _run = runs.begin(id).unwrap();
        runs.end(id);
        assert!(runs.begin(id).is_ok());
    }

    #[tokio::test]
    async fn a_subscriber_sees_the_backlog_then_live_events_with_no_gap_or_duplicate() {
        let run = Run::new(ConversationId(1));
        run.publish(step_start(0));

        // `AgentEvent` derives no `PartialEq` (it carries `AgentTurn`, which
        // doesn't either), so this matches structurally instead.
        let (backlog, mut sub) = run.subscribe();
        assert_eq!(backlog.len(), 1);
        assert!(matches!(backlog[0], AgentEvent::StepStart { step: 0 }));

        run.publish(step_start(1));
        let next = sub.recv().await.unwrap();
        assert!(matches!(next, AgentEvent::StepStart { step: 1 }));
    }

    #[tokio::test]
    async fn finish_closes_every_subscriber_and_exposes_the_status() {
        let run = Run::new(ConversationId(1));
        let (_backlog, mut sub) = run.subscribe();
        run.finish(done_status());
        assert!(sub.recv().await.is_none());
        assert_eq!(run.status(), Some(done_status()));
    }

    #[tokio::test]
    async fn dropping_a_subscription_deregisters_its_slot() {
        let run = Run::new(ConversationId(1));
        let (_backlog, sub) = run.subscribe();
        assert_eq!(run.state.lock().unwrap().subscribers.len(), 1);
        drop(sub);
        assert_eq!(run.state.lock().unwrap().subscribers.len(), 0);

        // Publishing afterwards must not panic or reach the dropped slot.
        run.publish(step_start(0));
    }

    #[tokio::test]
    async fn two_concurrent_followers_both_see_every_event_independently() {
        let run = Run::new(ConversationId(1));
        let (_b1, sub1) = run.subscribe();
        let (_b2, mut sub2) = run.subscribe();

        run.publish(step_start(0));
        drop(sub1);
        run.publish(step_start(1));

        assert!(matches!(sub2.recv().await.unwrap(), AgentEvent::StepStart { step: 0 }));
        assert!(matches!(sub2.recv().await.unwrap(), AgentEvent::StepStart { step: 1 }));
    }

    #[tokio::test]
    async fn a_subscriber_whose_receiver_was_dropped_without_its_subscription_is_swept_on_publish() {
        let run = Run::new(ConversationId(1));
        let mut state = run.state.lock().unwrap();
        let (tx, rx) = mpsc::unbounded_channel::<AgentEvent>();
        let slot = state.next_slot;
        state.next_slot += 1;
        state.subscribers.insert(slot, tx);
        drop(rx); // receiver gone, but no `Subscription` ever removed the slot
        drop(state);

        assert_eq!(run.state.lock().unwrap().subscribers.len(), 1);
        run.publish(step_start(0));
        assert_eq!(run.state.lock().unwrap().subscribers.len(), 0, "the dead sender should be swept");
    }

    #[tokio::test]
    async fn follow_replays_the_backlog_then_the_live_tail_and_returns_the_terminal_status() {
        let run = Run::new(ConversationId(1));
        run.publish(step_start(0));

        let run_for_publisher = Arc::clone(&run);
        tokio::spawn(async move {
            run_for_publisher.publish(step_start(1));
            run_for_publisher.finish(done_status());
        });

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_for_emit = seen.clone();
        let status = follow(run, move |event| {
            seen_for_emit.lock().unwrap().push(event.clone());
            true
        })
        .await;

        assert_eq!(status, Some(done_status()));
        let seen = seen.lock().unwrap();
        assert!(matches!(seen[0], AgentEvent::StepStart { step: 0 }));
        assert!(matches!(seen[1], AgentEvent::StepStart { step: 1 }));
    }

    #[tokio::test]
    async fn follow_stops_and_returns_none_as_soon_as_emit_declines() {
        let run = Run::new(ConversationId(1));
        run.publish(step_start(0));
        run.publish(step_start(1));

        let status = follow(Arc::clone(&run), |_event| false).await;
        assert_eq!(status, None);
        // The declining follower's subscription must have been dropped —
        // publishing afterwards should not find its slot.
        assert_eq!(run.state.lock().unwrap().subscribers.len(), 0);
    }
}
