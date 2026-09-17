//! The admin-only "All rooms" browser: a searchable, paged list of every live room, reachable
//! from both the Solo menu and the in-room view of the Multiplayer dialog (`ui::room`). Built on
//! `crate::rooms::RoomAdmin` the same way `ui::config`'s access-code table is built on
//! `crate::access::Access`.

use crate::access::Access;
use crate::rooms::RoomAdmin;
use crate::ui::config::format_timestamp;
use crate::ui::glossary::Topic;
use crate::ui::room::{copy_invite_link, RoomsOpen};
use crate::ui::{Modal, Spinner, Tip};
use leptos::prelude::*;
use leptos::web_sys;
use shared::rooms::RoomSummary;

/// Which room (if any) is pending a close confirmation -- dialog-local, not part of `RoomAdmin`
/// itself, mirroring `ui::config`'s `ConfirmingDelete`.
#[derive(Clone, Copy)]
struct ConfirmingClose(RwSignal<Option<String>>);

/// The one revealed-link field shared by every row's "Copy link" button (see `ui::room::
/// copy_invite_link`'s own doc comment on why `working`/`copied` carry *which* room they're
/// about) -- there is only ever one link built at a time, so one field is enough regardless of
/// how many rooms are on screen.
#[derive(Clone, Copy)]
struct RoomsLink {
    url: RwSignal<Option<String>>,
    copied: RwSignal<Option<String>>,
    working: RwSignal<Option<String>>,
}

impl RoomsLink {
    fn new() -> Self {
        Self { url: RwSignal::new(None), copied: RwSignal::new(None), working: RwSignal::new(None) }
    }
}

#[component]
pub fn AllRoomsButton() -> impl IntoView {
    let access = expect_context::<Access>();
    let open = expect_context::<RoomsOpen>().0;
    let is_admin = move || access.me().get().is_some_and(|code| code.is_admin);

    view! {
        {move || is_admin().then(|| view! {
            <Tip topic=Topic::RoomAdminList>
                <button class="btn" on:click=move |_| open.set(true)>"All rooms"</button>
            </Tip>
        })}
    }
}

#[component]
pub fn AllRoomsModal() -> impl IntoView {
    let open = expect_context::<RoomsOpen>().0;

    view! {
        {move || open.get().then(|| view! {
            <Modal title="All rooms" wide=true on_close=move || open.set(false)>
                <AllRoomsPanel />
            </Modal>
        })}
    }
}

/// Built fresh every time the modal opens (`AllRoomsModal` only ever mounts this while `open` is
/// true), so `RoomAdmin::new()` always starts from a live fetch rather than a stale one left over
/// from a previous visit.
#[component]
fn AllRoomsPanel() -> impl IntoView {
    let admin = RoomAdmin::new();
    provide_context(admin);
    provide_context(RoomsLink::new());
    provide_context(ConfirmingClose(RwSignal::new(None)));

    view! {
        <div class="rooms-admin">
            <input
                class="rooms-search"
                placeholder="Search rooms\u{2026}"
                prop:value=move || admin.search().get()
                on:input=move |ev| admin.set_search(event_target_value(&ev))
            />
            <div class="access-table-scroll">
                <table class="access-table">
                    <thead>
                        <tr>
                            <th>"Room"</th>
                            <th>"Members"</th>
                            <th>"Last active"</th>
                            <th><Tip topic=Topic::RoomInviteLink><span>"Link"</span></Tip></th>
                            <th><Tip topic=Topic::RoomAdminClose><span>"Close"</span></Tip></th>
                        </tr>
                    </thead>
                    <tbody>
                        <For each=move || admin.rooms().get() key=|room| room.display_name.to_string() let:room>
                            <RoomRow room=room />
                        </For>
                    </tbody>
                </table>
            </div>
            {move || (admin.rooms().get().is_empty() && !admin.busy().get()).then(|| view! {
                <p class="rooms-empty">"No rooms found."</p>
            })}
            {move || admin.busy().get().then(|| view! {
                <div class="room-pending"><Spinner /></div>
            })}
            {move || admin.has_more().get().then(|| view! {
                <button class="btn rooms-more" disabled=move || admin.busy().get() on:click=move |_| admin.load_more()>
                    "Show more"
                </button>
            })}
            <RevealedLink />
            <ConfirmCloseRoom />
        </div>
    }
}

#[component]
fn RoomRow(room: RoomSummary) -> impl IntoView {
    let access = expect_context::<Access>();
    let link = expect_context::<RoomsLink>();
    let confirming = expect_context::<ConfirmingClose>().0;

    let name = room.display_name.to_string();
    let updated_at = format_timestamp(*room.updated_at);
    let member_count = room.member_count;

    let is_working = {
        let name = name.clone();
        Signal::derive(move || link.working.get().as_deref() == Some(name.as_str()))
    };
    let is_copied = {
        let name = name.clone();
        Signal::derive(move || link.copied.get().as_deref() == Some(name.as_str()))
    };
    let copy = {
        let name = name.clone();
        move |_| copy_invite_link(access, name.clone(), link.url, link.copied, link.working)
    };
    let close = {
        let name = name.clone();
        move |_| confirming.set(Some(name.clone()))
    };

    view! {
        <tr>
            <td class="access-key">{name.clone()}</td>
            <td>{member_count.to_string()}</td>
            <td class="access-created">{updated_at}</td>
            <td>
                <button class="btn" on:click=copy disabled=move || is_working.get()>
                    {move || match (is_working.get(), is_copied.get()) {
                        (true, _) => "Preparing\u{2026}",
                        (false, true) => "Copied!",
                        (false, false) => "Copy link",
                    }}
                </button>
            </td>
            <td>
                <button class="btn access-delete" on:click=close>"\u{2715}"</button>
            </td>
        </tr>
    }
}

/// The clipboard fallback for whichever row's link was last built -- an insecure origin (see
/// `ui::room::copy_to_clipboard`'s own doc comment) never writes to the clipboard at all, so this
/// is the only way to actually get at the link there.
#[component]
fn RevealedLink() -> impl IntoView {
    let link = expect_context::<RoomsLink>();

    view! {
        {move || link.url.get().map(|url| view! {
            <input
                class="room-invite-url"
                readonly=true
                prop:value=url
                on:click=move |ev| { event_target::<web_sys::HtmlInputElement>(&ev).select(); }
            />
        })}
    }
}

#[component]
fn ConfirmCloseRoom() -> impl IntoView {
    let admin = expect_context::<RoomAdmin>();
    let confirming = expect_context::<ConfirmingClose>().0;
    let busy = admin.busy();

    view! {
        {move || {
            confirming.get().map(|room_name| {
                let confirm_name = room_name.clone();
                view! {
                    <Modal title="Close room?" on_close=move || confirming.set(None)>
                        <p class="reset-warning">
                            "This immediately deletes \"" {room_name.clone()}
                            "\" and disconnects everyone currently in it. It can't be undone."
                        </p>
                        <div class="reset-actions">
                            <button class="btn" on:click=move |_| confirming.set(None)>"Cancel"</button>
                            <button
                                class="btn reset-confirm"
                                disabled=move || busy.get()
                                on:click=move |_| admin.delete(confirm_name.clone(), move || confirming.set(None))
                            >
                                "Close room"
                            </button>
                        </div>
                    </Modal>
                }
            })
        }}
    }
}
