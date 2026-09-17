//! The one facade every UI call site uses to check or manage access codes, in the shape of
//! `battle_net.rs`'s `Battles`: signals for what's known, methods that spawn their own requests
//! against `crate::api`, and a single `busy()` flag every mutating action shares. `start` is the
//! exception to that last point -- see its own doc comment -- and is `crate::startup`'s entry
//! point into this module.

use crate::api::{self, ApiError};
use crate::persist::Persisted;
use leptos::prelude::*;
use shared::access::{AccessCode, CreateAccessCode};
use std::future::Future;

fn sort_newest_first(codes: &mut [AccessCode]) {
    codes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
}

/// What `/auth/me` said about a stored code, reduced to the three cases `stored_plan`
/// distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckOutcome {
    Valid,
    Revoked,
    Unreachable,
}

/// What to do with a stored code once `/auth/me` has answered for it -- kept as its own pure
/// function so the policy has a single, tested home instead of being buried in `resolve`'s
/// branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredPlan {
    Keep,
    Drop { try_incoming: bool },
}

fn stored_plan(outcome: CheckOutcome, has_incoming: bool) -> StoredPlan {
    match outcome {
        CheckOutcome::Valid => StoredPlan::Keep,
        CheckOutcome::Revoked => StoredPlan::Drop { try_incoming: has_incoming },
        // A failure to *reach* the server is not a revocation: burning a good code on a network
        // blip would sign someone out of their own browser because their wifi hiccuped, and
        // adopting a link's code on the strength of it would let an invite silently replace a
        // working code just by arriving during an outage.
        CheckOutcome::Unreachable => StoredPlan::Keep,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InviteCodeError {
    #[error("not signed in")]
    NotSignedIn,
    #[error("there is no non-admin access code to invite with; create one in Settings")]
    NoGuestCode,
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[derive(Clone, Copy)]
pub struct Access {
    token: Persisted<Option<String>>,
    me: RwSignal<Option<AccessCode>>,
    codes: RwSignal<Vec<AccessCode>>,
    in_flight: RwSignal<bool>,
    error: RwSignal<Option<String>>,
}

impl Access {
    pub fn new() -> Self {
        Self {
            token: Persisted::new("auth.token", || None),
            me: RwSignal::new(None),
            codes: RwSignal::new(Vec::new()),
            in_flight: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn me(&self) -> Signal<Option<AccessCode>> {
        let me = self.me;
        Signal::derive(move || me.get())
    }

    /// The raw bearer token, for whatever wants to authenticate its own requests directly (the
    /// websocket client, in particular — every message it sends carries this). `None` means "not
    /// signed in," the same condition `me()` reports.
    pub fn token(&self) -> Signal<Option<String>> {
        let token = self.token;
        Signal::derive(move || token.get())
    }

    pub fn codes(&self) -> Signal<Vec<AccessCode>> {
        let codes = self.codes;
        Signal::derive(move || codes.get())
    }

    /// The single flag every form in the settings dialog disables on and shows one spinner for.
    pub fn busy(&self) -> Signal<bool> {
        let in_flight = self.in_flight;
        Signal::derive(move || in_flight.get())
    }

    /// The last failure worth showing inline in the "no code" prompt -- in particular, why a
    /// previously working code stopped working (see `resolve`'s `Revoked` handling).
    pub fn error(&self) -> Signal<Option<String>> {
        let error = self.error;
        Signal::derive(move || error.get())
    }

    /// Checks `token` against `/auth/me` and, only once that succeeds, persists it and loads the
    /// admin table if it's an admin code. A bad code is reported inline instead of ever being
    /// stored, so a typo never silently overwrites a working saved code.
    pub fn sign_in(&self, token: String) {
        let this = *self;
        self.run_reporting_inline("check access code", async move {
            let code = api::me(&token).await?;
            let is_admin = code.is_admin;
            let codes = if is_admin { fetch_sorted(&token).await? } else { Vec::new() };
            this.token.set(Some(token));
            this.me.set(Some(code));
            this.codes.set(codes);
            Ok(())
        });
    }

    pub fn clear(&self) {
        self.token.set(None);
        self.me.set(None);
        self.codes.set(Vec::new());
        self.error.set(None);
    }

    /// Settles which access code this browser uses, once, at startup: `run_reporting_inline` and
    /// `run_toasting` both no-op while `in_flight` is already set (see their own doc comments), so
    /// checking a stored code and then falling back to a link's code as two separate calls would
    /// silently drop the second -- this does the whole thing as one task instead. `incoming` is an
    /// `auth_code` query parameter a link brought in (see `crate::link`), adopted only if this
    /// browser doesn't already hold a code the server still accepts. `then` is handed whether
    /// there is now a usable code -- all `crate::startup` needs to decide whether a room can be
    /// entered at all. Deliberately has no `in_flight` guard of its own: this is the first thing
    /// that ever runs, so a guard here could only ever report a spurious `false`.
    pub fn start(&self, incoming: Option<String>, then: impl FnOnce(bool) + 'static) {
        self.in_flight.set(true);
        let this = *self;
        leptos::task::spawn_local_scoped(async move {
            let signed_in = this.resolve(incoming).await;
            this.in_flight.set(false);
            then(signed_in);
        });
    }

    async fn resolve(self, incoming: Option<String>) -> bool {
        let Some(stored) = self.token.get_untracked() else {
            return match incoming {
                Some(token) => self.adopt(token).await,
                None => false,
            };
        };

        let outcome = match api::me(&stored).await {
            Ok(code) => {
                self.accept(stored, code).await;
                CheckOutcome::Valid
            }
            Err(ApiError::Unauthorized) => CheckOutcome::Revoked,
            Err(error) => {
                tracing::error!(%error, "could not check the saved access code");
                crate::ui::toast::error(format!("Could not check your saved access code: {error}"));
                CheckOutcome::Unreachable
            }
        };

        match stored_plan(outcome, incoming.is_some()) {
            StoredPlan::Keep => true,
            StoredPlan::Drop { try_incoming } => {
                self.clear();
                self.error.set(Some("That access code is no longer valid.".to_string()));
                match incoming.filter(|_| try_incoming) {
                    Some(token) => self.adopt(token).await,
                    None => false,
                }
            }
        }
    }

    /// Checks a fresh code -- an incoming `auth_code`, never a stored one; `resolve` handles those
    /// itself -- and adopts it on success.
    async fn adopt(self, token: String) -> bool {
        match api::me(&token).await {
            Ok(code) => {
                self.accept(token, code).await;
                true
            }
            Err(error) => {
                tracing::error!(%error, "could not check the invited access code");
                crate::ui::toast::error(format!("That invite link's access code didn't work: {error}"));
                false
            }
        }
    }

    /// Publishes a code `/auth/me` has already confirmed. `me` is set *before* the admin table is
    /// fetched (unlike `sign_in`'s own inline `?`), so a `/access-codes` failure never leaves an
    /// admin looking signed out over a request nothing here actually depends on succeeding.
    async fn accept(self, token: String, code: AccessCode) {
        let is_admin = code.is_admin;
        self.token.set(Some(token.clone()));
        self.me.set(Some(code));
        self.error.set(None);
        if !is_admin {
            self.codes.set(Vec::new());
            return;
        }
        match fetch_sorted(&token).await {
            Ok(codes) => self.codes.set(codes),
            Err(error) => {
                tracing::error!(%error, "could not load the access-code table");
                crate::ui::toast::error(format!("Could not load the access-code table: {error}"));
            }
        }
    }

    pub fn create(&self, access_key: Option<String>, is_admin: bool) {
        let Some(token) = self.token.get_untracked() else { return };
        let access_key =
            access_key.map(|key| key.try_into().expect("non-empty by construction: ui/config.rs filters blanks"));
        let this = *self;
        self.run_toasting("create access code", async move {
            api::create(&token, &CreateAccessCode { access_key, is_admin }).await?;
            this.codes.set(fetch_sorted(&token).await?);
            Ok(())
        });
    }

    pub fn set_admin(&self, access_key: String, is_admin: bool) {
        let Some(token) = self.token.get_untracked() else { return };
        let this = *self;
        self.run_toasting("update access code", async move {
            api::update(&token, &access_key, is_admin).await?;
            this.codes.set(fetch_sorted(&token).await?);
            Ok(())
        });
    }

    pub fn delete(&self, access_key: String) {
        let Some(token) = self.token.get_untracked() else { return };
        let this = *self;
        self.run_toasting("delete access code", async move {
            api::delete(&token, &access_key).await?;
            this.codes.set(fetch_sorted(&token).await?);
            Ok(())
        });
    }

    /// The access code an invite link should carry: this browser's own, unless it's an admin
    /// code -- an invite must never hand out the right to manage every other code, and an admin's
    /// own code is the one code it must never share -- in which case the newest non-admin code the
    /// server knows about stands in for it. Deliberately outside `busy()`/`in_flight`: a settings-
    /// dialog request already in flight must not make this button silently do nothing, and this
    /// never mutates any of `Access`'s own signals, so there's nothing for the guard to protect.
    /// Fetches fresh rather than reading the cached `codes()` -- that's only ever as current as
    /// the last settings-dialog action, and a code made in another tab wouldn't be in it.
    pub fn invite_code(&self, then: impl FnOnce(Result<String, InviteCodeError>) + 'static) {
        let Some(token) = self.token.get_untracked() else {
            then(Err(InviteCodeError::NotSignedIn));
            return;
        };
        let Some(code) = self.me.get_untracked() else {
            then(Err(InviteCodeError::NotSignedIn));
            return;
        };
        if !code.is_admin {
            then(Ok(token));
            return;
        }
        leptos::task::spawn_local_scoped(async move {
            let result = fetch_sorted(&token).await;
            then(match result {
                Ok(codes) => codes
                    .into_iter()
                    .find(|code| !code.is_admin)
                    .map(|code| code.access_key.to_string())
                    .ok_or(InviteCodeError::NoGuestCode),
                Err(error) => Err(InviteCodeError::Api(error)),
            });
        });
    }

    /// Reports failure inline via `error()`, and clears it on success -- for `sign_in`, whose
    /// failure is the very thing the prompt it's submitted from needs to display.
    fn run_reporting_inline(&self, action: &'static str, task: impl Future<Output = Result<(), ApiError>> + 'static) {
        if self.in_flight.get_untracked() {
            return;
        }
        self.in_flight.set(true);
        let this = *self;
        leptos::task::spawn_local_scoped(async move {
            let result = task.await;
            this.in_flight.set(false);
            match result {
                Ok(()) => this.error.set(None),
                Err(error) => {
                    tracing::error!(%error, "could not {action}");
                    this.error.set(Some(error.to_string()));
                }
            }
        });
    }

    /// Reports failure via the toast layer and otherwise leaves `error` alone -- for everything
    /// already showing a result (metadata, the admin table), where the failure is unexpected
    /// rather than the point of the action. `resolve`'s `Revoked` case sets `error` itself, as
    /// part of what it reports as success -- this must not clear that back out.
    fn run_toasting(&self, action: &'static str, task: impl Future<Output = Result<(), ApiError>> + 'static) {
        if self.in_flight.get_untracked() {
            return;
        }
        self.in_flight.set(true);
        let this = *self;
        leptos::task::spawn_local_scoped(async move {
            let result = task.await;
            this.in_flight.set(false);
            if let Err(error) = result {
                tracing::error!(%error, "could not {action}");
                crate::ui::toast::error(format!("Could not {action}: {error}"));
            }
        });
    }
}

async fn fetch_sorted(token: &str) -> Result<Vec<AccessCode>, ApiError> {
    let mut codes = api::list(token).await?;
    sort_newest_first(&mut codes);
    Ok(codes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::timestamp::Timestamp;
    use time::OffsetDateTime;

    fn code_at(access_key: &str, unix: i64) -> AccessCode {
        AccessCode {
            access_key: access_key.try_into().unwrap(),
            is_admin: false,
            created_at: Timestamp(OffsetDateTime::from_unix_timestamp(unix).unwrap()),
        }
    }

    #[test]
    fn sorts_newest_first() {
        let mut codes = vec![code_at("a", 100), code_at("b", 300), code_at("c", 200)];
        sort_newest_first(&mut codes);
        assert_eq!(codes.iter().map(|code| code.access_key.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
    }

    #[test]
    fn a_valid_stored_code_is_kept_regardless_of_an_incoming_one() {
        assert_eq!(stored_plan(CheckOutcome::Valid, false), StoredPlan::Keep);
        assert_eq!(stored_plan(CheckOutcome::Valid, true), StoredPlan::Keep);
    }

    #[test]
    fn a_revoked_stored_code_falls_back_to_an_incoming_one_when_there_is_one() {
        assert_eq!(stored_plan(CheckOutcome::Revoked, true), StoredPlan::Drop { try_incoming: true });
        assert_eq!(stored_plan(CheckOutcome::Revoked, false), StoredPlan::Drop { try_incoming: false });
    }

    #[test]
    fn a_network_failure_never_burns_a_stored_code_or_adopts_an_incoming_one() {
        assert_eq!(stored_plan(CheckOutcome::Unreachable, false), StoredPlan::Keep);
        assert_eq!(stored_plan(CheckOutcome::Unreachable, true), StoredPlan::Keep);
    }
}
