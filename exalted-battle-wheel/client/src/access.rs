//! The one facade every UI call site uses to check or manage access codes, in the shape of
//! `battle_net.rs`'s `Battles`: signals for what's known, methods that spawn their own requests
//! against `crate::api`, and a single `busy()` flag every mutating action shares.

use crate::api::{self, ApiError};
use crate::persist::Persisted;
use leptos::prelude::*;
use shared::access::{AccessCode, CreateAccessCode};
use std::future::Future;

fn sort_newest_first(codes: &mut [AccessCode]) {
    codes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
        let access = Self {
            token: Persisted::new("auth.token", || None),
            me: RwSignal::new(None),
            codes: RwSignal::new(Vec::new()),
            in_flight: RwSignal::new(false),
            error: RwSignal::new(None),
        };
        if access.token.get_untracked().is_some() {
            access.refresh();
        }
        access
    }

    pub fn me(&self) -> Signal<Option<AccessCode>> {
        let me = self.me;
        Signal::derive(move || me.get())
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
    /// previously working code stopped working (see `refresh`'s `Unauthorized` handling).
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

    /// Re-checks a stored code -- called once on startup if one was persisted. If the server no
    /// longer recognizes it (revoked or deleted from another session), the stored code is dropped
    /// and `error` explains why, so the UI falls back to the same "no code" prompt `sign_in`'s
    /// failure uses rather than a distinct third state.
    pub fn refresh(&self) {
        let Some(token) = self.token.get_untracked() else { return };
        let this = *self;
        self.run_toasting("check access code", async move {
            match api::me(&token).await {
                Ok(code) => {
                    let is_admin = code.is_admin;
                    let codes = if is_admin { fetch_sorted(&token).await? } else { Vec::new() };
                    this.me.set(Some(code));
                    this.codes.set(codes);
                    Ok(())
                }
                Err(ApiError::Unauthorized) => {
                    this.token.set(None);
                    this.me.set(None);
                    this.codes.set(Vec::new());
                    this.error.set(Some("That access code is no longer valid.".to_string()));
                    Ok(())
                }
                Err(error) => Err(error),
            }
        });
    }

    pub fn create(&self, access_key: Option<String>, is_admin: bool) {
        let Some(token) = self.token.get_untracked() else { return };
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
    /// rather than the point of the action. `refresh`'s `Unauthorized` case sets `error` itself,
    /// as part of what it reports as success -- this must not clear that back out.
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
    use time::OffsetDateTime;

    fn code_at(access_key: &str, unix: i64) -> AccessCode {
        AccessCode {
            access_key: access_key.to_string(),
            is_admin: false,
            created_at: OffsetDateTime::from_unix_timestamp(unix).unwrap(),
        }
    }

    #[test]
    fn sorts_newest_first() {
        let mut codes = vec![code_at("a", 100), code_at("b", 300), code_at("c", 200)];
        sort_newest_first(&mut codes);
        assert_eq!(codes.iter().map(|code| code.access_key.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
    }
}
