//! The admin-only room browser's reactive facade, in the shape of `crate::access::Access`:
//! signals for what's currently known plus methods that spawn their own requests against
//! `crate::api`, so `ui::rooms_admin` stays views-only. Reads the bearer token from `Access`
//! itself (via context) rather than owning a copy, the same way `battle_net.rs`'s `Battles` does.

use crate::access::Access;
use crate::api::{self, ApiError};
use leptos::prelude::*;
use shared::rooms::RoomSummary;
use std::time::Duration;

/// How long `set_search` waits for more typing before actually issuing a request -- without this,
/// every keystroke would be its own `GET /rooms` scan.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

/// `GET /rooms`'s page size this browser asks for. The server clamps regardless (see
/// `server::rooms::MAX_ROOM_PAGE`), so this only ever needs to be a reasonable default.
const PAGE_SIZE: usize = 20;

/// Whether a response that was started under `started_generation` should still be applied, given
/// the counter now reads `current_generation`. Typing further, an explicit refresh, and a
/// successful delete's own refresh all bump the counter -- so a response for a generation that's
/// since moved on is simply dropped instead of clobbering a newer one that may have already
/// landed (network order gives no guarantee the two arrive in the order they were sent).
fn is_stale(current_generation: u64, started_generation: u64) -> bool {
    current_generation != started_generation
}

/// What a fetched page does to the rooms already on screen: `load_more` appends onto what's
/// there, everything else (a fresh search, an explicit refresh, the page after a delete) replaces
/// it outright.
fn merge_page(existing: Vec<RoomSummary>, fetched: Vec<RoomSummary>, append: bool) -> Vec<RoomSummary> {
    if !append {
        return fetched;
    }
    let mut merged = existing;
    merged.extend(fetched);
    merged
}

#[derive(Clone, Copy)]
pub struct RoomAdmin {
    search: RwSignal<String>,
    rooms: RwSignal<Vec<RoomSummary>>,
    next_cursor: RwSignal<Option<String>>,
    in_flight: RwSignal<bool>,
    /// Bumped by every search, refresh, and post-delete refresh -- see `is_stale`.
    generation: RwSignal<u64>,
}

impl RoomAdmin {
    pub fn new() -> Self {
        let this = Self {
            search: RwSignal::new(String::new()),
            rooms: RwSignal::new(Vec::new()),
            next_cursor: RwSignal::new(None),
            in_flight: RwSignal::new(false),
            generation: RwSignal::new(0),
        };
        this.fetch(false);
        this
    }

    pub fn search(&self) -> Signal<String> {
        let search = self.search;
        Signal::derive(move || search.get())
    }

    pub fn rooms(&self) -> Signal<Vec<RoomSummary>> {
        let rooms = self.rooms;
        Signal::derive(move || rooms.get())
    }

    pub fn busy(&self) -> Signal<bool> {
        let in_flight = self.in_flight;
        Signal::derive(move || in_flight.get())
    }

    pub fn has_more(&self) -> Signal<bool> {
        let next_cursor = self.next_cursor;
        Signal::derive(move || next_cursor.get().is_some())
    }

    /// Updates the search box immediately -- typing must never feel like it's waiting on a
    /// network round trip -- and, after `SEARCH_DEBOUNCE` of no further calls, refetches page one
    /// under the new query.
    pub fn set_search(&self, query: String) {
        self.search.set(query);
        let this = *self;
        let my_generation = this.generation.get_untracked() + 1;
        this.generation.set(my_generation);
        set_timeout(
            move || {
                if this.generation.get_untracked() == my_generation {
                    this.fetch(false);
                }
            },
            SEARCH_DEBOUNCE,
        );
    }

    /// Refetches page one under the current search, right now -- for the "Show more" button's
    /// sibling actions and for a delete's own follow-up (see `delete` below).
    pub fn refresh(&self) {
        self.generation.update(|generation| *generation += 1);
        self.fetch(false);
    }

    /// Fetches the next page under the current search and appends it. A no-op while a fetch is
    /// already in flight or there's no further page -- `ui::rooms_admin`'s "Show more" button is
    /// disabled in both cases, but a caller here shouldn't have to know that.
    pub fn load_more(&self) {
        if self.in_flight.get_untracked() || self.next_cursor.get_untracked().is_none() {
            return;
        }
        self.fetch(true);
    }

    fn fetch(&self, append: bool) {
        let Some(token) = expect_context::<Access>().token().get_untracked() else { return };
        let this = *self;
        let my_generation = this.generation.get_untracked();
        let search = this.search.get_untracked();
        let cursor = if append { this.next_cursor.get_untracked() } else { None };
        this.in_flight.set(true);
        leptos::task::spawn_local_scoped(async move {
            let result = api::rooms(&token, &search, PAGE_SIZE, cursor.as_deref()).await;
            if is_stale(this.generation.get_untracked(), my_generation) {
                return;
            }
            this.in_flight.set(false);
            match result {
                Ok(list) => {
                    this.rooms.update(|rooms| *rooms = merge_page(std::mem::take(rooms), list.rooms, append));
                    this.next_cursor.set(list.next_cursor);
                }
                Err(error) => {
                    tracing::error!(%error, "could not load rooms");
                    crate::ui::toast::error(format!("Could not load rooms: {error}"));
                }
            }
        });
    }

    /// Closes a room outright and, on success, refetches page one under the current search --
    /// simpler than trying to patch the deleted room out of whatever page happened to be showing,
    /// and correct even if the delete changed which rooms belong on that page at all.
    pub fn delete(&self, room_name: String, then: impl FnOnce() + 'static) {
        let Some(token) = expect_context::<Access>().token().get_untracked() else { return };
        let this = *self;
        this.in_flight.set(true);
        leptos::task::spawn_local_scoped(async move {
            let result: Result<(), ApiError> = api::delete_room(&token, &room_name).await;
            this.in_flight.set(false);
            match result {
                Ok(()) => {
                    then();
                    this.refresh();
                }
                Err(error) => {
                    tracing::error!(%error, "could not close room");
                    crate::ui::toast::error(format!("Could not close room: {error}"));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_response_from_the_current_generation_is_not_stale() {
        assert!(!is_stale(3, 3));
    }

    #[test]
    fn a_response_from_an_earlier_generation_is_stale() {
        assert!(is_stale(4, 3));
    }

    fn room(name: &str) -> RoomSummary {
        RoomSummary { display_name: name.try_into().unwrap(), member_count: 0, updated_at: shared::timestamp::Timestamp(time::OffsetDateTime::now_utc()) }
    }

    #[test]
    fn a_fresh_page_replaces_whatever_was_showing() {
        let existing = vec![room("old")];
        let fetched = vec![room("new")];
        let merged = merge_page(existing, fetched, false);
        assert_eq!(merged.iter().map(|room| room.display_name.to_string()).collect::<Vec<_>>(), ["new"]);
    }

    #[test]
    fn load_more_appends_onto_what_was_already_there() {
        let existing = vec![room("first")];
        let fetched = vec![room("second")];
        let merged = merge_page(existing, fetched, true);
        assert_eq!(merged.iter().map(|room| room.display_name.to_string()).collect::<Vec<_>>(), ["first", "second"]);
    }
}
