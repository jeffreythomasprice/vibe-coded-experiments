//! Host or join a direct peer-to-peer room: name yourself, exchange connection codes, see who's in
//! the room, rename yourself at any time, and — if you're an admin — promote or demote anyone but
//! yourself and the host, or (host only) kick someone. Joining adopts the host's whole battle
//! immediately; from then on every edit anyone makes is a two-phase-commit vote riding the same
//! connection, so the room stays in agreement or disconnects loudly rather than drifting apart.
//!
//! There is no server: whoever hosts shares a connection code out of band (chat, a call), and the
//! other side pastes back a reply code the same way. Anyone with the code can join. Admin is
//! enforced against anything that reaches the shared battle — the host refuses a change proposed
//! by a non-admin — but a connected peer still votes on every change like everyone else, so a
//! modified client can still stall or tear down the room by voting badly.

use crate::battle_net::Battles;
use crate::net::{Mode, Role};
use crate::prefs::Prefs;
use crate::ui::glossary::Topic;
use crate::ui::{CopyButton, DetailTip, Modal, Spinner, Tip};
use leptos::prelude::*;
use shared::protocol::PeerId;

/// Which sub-view `Mode::Solo` is showing. Unlike the room's actual state (host/joined/peers,
/// all owned by `Session`), this is purely local UI navigation with nothing to keep in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuChoice {
    Root,
    Joining,
}

/// An invite code carried in on this page load's `#j=` URL fragment (see `app.rs`, which parses
/// it once at boot and clears the fragment). `RoomButton` consumes it on its very first render to
/// open straight into the join view with the code already filled in, then clears it here too so
/// navigating the modal closed and back open doesn't resurrect a stale invite.
#[derive(Clone, Copy)]
pub struct PendingJoin(pub RwSignal<Option<String>>);

#[component]
pub fn RoomButton() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let pending_join = expect_context::<PendingJoin>();
    let open = RwSignal::new(pending_join.0.with_untracked(Option::is_some));

    let label = move || match battles.mode().get() {
        Mode::Solo => "Multiplayer (Solo)".to_string(),
        Mode::Hosting => format!("Multiplayer (Hosting \u{2014} {})", battles.peers().get().len()),
        Mode::Joined => "Multiplayer (Joined)".to_string(),
    };

    view! {
        <Tip topic=Topic::Room>
            <button on:click=move |_| open.set(true)>{label}</button>
        </Tip>
        {move || {
            open.get().then(|| view! {
                <Modal title="Multiplayer" on_close=move || open.set(false)>
                    <RoomPanelBody />
                </Modal>
            })
        }}
    }
}

#[component]
fn RoomPanelBody() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let prefs = expect_context::<Prefs>();
    let pending_join = expect_context::<PendingJoin>();
    // Consumed here, once, at this component's own setup — it only ever mounts fresh when the
    // modal opens, which for a pending join is the very render `RoomButton` triggered above.
    let initial_join_code = pending_join.0.get_untracked();
    pending_join.0.set(None);

    let menu = RwSignal::new(if initial_join_code.is_some() { MenuChoice::Joining } else { MenuChoice::Root });
    let everyone_admin = RwSignal::new(true);
    let offer_input = RwSignal::new(initial_join_code.unwrap_or_default());

    let host = move |_| {
        let name = prefs.player_name.get();
        if name.trim().is_empty() {
            return;
        }
        battles.host(name.trim().to_string(), everyone_admin.get());
    };

    let join = move |_| {
        let name = prefs.player_name.get();
        let offer = offer_input.get();
        if name.trim().is_empty() || offer.trim().is_empty() {
            return;
        }
        battles.join(offer.trim().to_string(), name.trim().to_string());
    };

    let leave = move |_| {
        battles.leave();
        menu.set(MenuChoice::Root);
    };

    view! {
        <div class="room-panel">
            {move || match battles.mode().get() {
                Mode::Solo => view! {
                    <div class="room-menu">
                        <NameField />
                        {move || match menu.get() {
                            MenuChoice::Root => view! {
                                <>
                                    <Tip topic=Topic::RoomAdminModel>
                                        <label class="room-field-inline">
                                            <input
                                                type="checkbox"
                                                prop:checked=move || everyone_admin.get()
                                                on:change=move |ev| everyone_admin.set(event_target_checked(&ev))
                                            />
                                            "Everyone who joins starts as an admin"
                                        </label>
                                    </Tip>
                                    <button class="btn" on:click=host>"Host a room"</button>
                                    <button class="btn" on:click=move |_| menu.set(MenuChoice::Joining)>"Join a room"</button>
                                </>
                            }.into_any(),
                            MenuChoice::Joining => view! {
                                <>
                                    <label class="room-field">
                                        "Paste the host's connection code here"
                                        <textarea
                                            prop:value=move || offer_input.get()
                                            on:input=move |ev| offer_input.set(event_target_value(&ev))
                                        />
                                    </label>
                                    <button class="btn" on:click=join disabled=move || offer_input.get().trim().is_empty()>
                                        "Join"
                                    </button>
                                    <button class="btn" on:click=move |_| menu.set(MenuChoice::Root)>"Back"</button>
                                </>
                            }.into_any(),
                        }}
                    </div>
                }.into_any(),
                Mode::Hosting => view! {
                    <div class="room-menu">
                        <NameField />
                        <HostingFields />
                        <PeerList />
                        {move || battles.awaiting_peer().get().then(|| view! {
                            <div class="room-pending"><Spinner /> "Waiting for a player to join\u{2026}"</div>
                        })}
                        <button class="btn" on:click=leave>"Leave room"</button>
                    </div>
                }.into_any(),
                Mode::Joined => view! {
                    <div class="room-menu">
                        <NameField />
                        <JoinedFields />
                        <PeerList />
                        {move || battles.self_id().get().is_none().then(|| view! {
                            <div class="room-pending"><Spinner /> "Joining room\u{2026}"</div>
                        })}
                        <button class="btn" on:click=leave>"Leave room"</button>
                    </div>
                }.into_any(),
            }}
            <Advanced />
        </div>
    }
}

/// The same field in all three modes. `prefs.player_name` is this browser's own copy and follows
/// every keystroke; the room only hears about a rename on `on:change` (blur or Enter), since every
/// accepted one costs the host a full roster rebroadcast to every peer, not worth paying per
/// keystroke on a channel also carrying the agreement traffic. `battles.rename` is a no-op outside
/// a room, so this one component serves `Solo`, `Hosting`, and `Joined` alike.
#[component]
fn NameField() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let prefs = expect_context::<Prefs>();

    view! {
        <label class="room-field">
            <Tip topic=Topic::RoomRename><span>"Your name"</span></Tip>
            <input
                placeholder="Name"
                prop:value=move || prefs.player_name.get()
                on:input=move |ev| prefs.player_name.set(event_target_value(&ev))
                on:change=move |ev| battles.rename(event_target_value(&ev))
            />
        </label>
    }
}

/// One editable STUN server entry, keyed by a locally-minted id (not the URL itself) so `<For>`
/// survives edits and reorders while a row is empty or invalid.
#[derive(Clone, Copy)]
struct StunRow {
    row_id: u32,
    url: RwSignal<String>,
}

fn next_stun_row_id(counter: RwSignal<u32>) -> u32 {
    let id = counter.get_untracked();
    counter.set(id + 1);
    id
}

fn stun_rows_from(urls: Vec<String>, counter: RwSignal<u32>) -> Vec<StunRow> {
    urls.into_iter().map(|url| StunRow { row_id: next_stun_row_id(counter), url: RwSignal::new(url) }).collect()
}

#[component]
fn Advanced() -> impl IntoView {
    let open = RwSignal::new(false);

    view! {
        <div class="room-advanced">
            <Tip topic=Topic::RoomStunServers>
                <button class="room-advanced-toggle" on:click=move |_| open.update(|open| *open = !*open)>
                    {move || if open.get() { "\u{25be} Advanced" } else { "\u{25b8} Advanced" }}
                </button>
            </Tip>
            {move || open.get().then(|| view! { <StunServerList /> })}
        </div>
    }
}

#[component]
fn StunServerList() -> impl IntoView {
    let row_counter = RwSignal::new(0u32);
    let rows = RwSignal::new(stun_rows_from(crate::net::stun_servers(), row_counter));

    // Read every row's `url` unconditionally (not just the ones that fail to validate), so this
    // effect tracks every row and re-runs on any keystroke in any of them, not just the first
    // invalid one it happens to see.
    Effect::new(move |_| {
        let urls: Vec<String> = rows.get().iter().map(|row| row.url.get()).collect();
        if !urls.is_empty() && urls.iter().all(|url| crate::net::validate_stun_url(url).is_ok()) {
            crate::net::set_stun_servers(urls);
        }
    });

    let add_row = move |_| {
        let row_id = next_stun_row_id(row_counter);
        rows.update(|rows| rows.push(StunRow { row_id, url: RwSignal::new(String::new()) }));
    };
    let remove_row = move |row_id: u32| rows.update(|rows| rows.retain(|row| row.row_id != row_id));
    let restore_defaults = move |_| rows.set(stun_rows_from(crate::net::default_stun_servers(), row_counter));

    view! {
        <div class="room-stun-list">
            <For each=move || rows.get() key=|row| row.row_id let:row>
                <StunServerRow row=row on_remove=remove_row />
            </For>
            {move || rows.get().is_empty().then(|| view! {
                <p class="room-error">"No servers listed \u{2014} add at least one, or the previous list stays in effect."</p>
            })}
            <div class="room-stun-actions">
                <button class="btn" on:click=add_row>"Add server"</button>
                <button class="btn" on:click=restore_defaults>"Restore defaults"</button>
            </div>
        </div>
    }
}

#[component]
fn StunServerRow(row: StunRow, on_remove: impl Fn(u32) + Copy + 'static) -> impl IntoView {
    let error = move || crate::net::validate_stun_url(&row.url.get()).err();

    view! {
        <>
            <div class="room-stun-row">
                <input prop:value=move || row.url.get() on:input=move |ev| row.url.set(event_target_value(&ev)) />
                <button class="btn" on:click=move |_| on_remove(row.row_id)>"\u{2715}"</button>
            </div>
            {move || error().map(|error| view! { <p class="room-error">{error.to_string()}</p> })}
        </>
    }
}

#[component]
fn HostingFields() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let answer_input = RwSignal::new(String::new());
    let preparing = move || battles.invite().get().is_none();

    let accept = move |_| {
        let code = answer_input.get();
        if code.trim().is_empty() {
            return;
        }
        battles.accept_answer(code.trim().to_string());
        answer_input.set(String::new());
    };

    view! {
        <>
            <div class="room-invite-slot">
                {move || match battles.invite().get() {
                    Some(code) => view! {
                        <div class="room-code">
                            <label class="room-field">
                                "Send this to whoever is joining"
                                <textarea readonly=true prop:value=code.clone() />
                            </label>
                            <CopyButton text=code />
                        </div>
                    }.into_any(),
                    None => view! {
                        <div class="room-pending"><Spinner /> "Preparing an invite\u{2026}"</div>
                    }.into_any(),
                }}
            </div>
            <div class:room-busy=preparing>
                <label class="room-field">
                    "Paste their reply code here"
                    <textarea
                        prop:value=move || answer_input.get()
                        on:input=move |ev| answer_input.set(event_target_value(&ev))
                        disabled=preparing
                    />
                </label>
                <button class="btn" on:click=accept disabled=move || preparing() || answer_input.get().trim().is_empty()>
                    "Connect"
                </button>
            </div>
        </>
    }
}

#[component]
fn JoinedFields() -> impl IntoView {
    let battles = expect_context::<Battles>();
    view! {
        <div class="room-invite-slot">
            {move || match battles.answer_code().get() {
                Some(code) => view! {
                    <div class="room-code">
                        <label class="room-field">
                            "Send this back to the host"
                            <textarea readonly=true prop:value=code.clone() />
                        </label>
                        <CopyButton text=code />
                    </div>
                }.into_any(),
                None => view! {
                    <div class="room-pending"><Spinner /> "Preparing your reply\u{2026}"</div>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn PeerList() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let peer_ids = move || battles.peers().get().iter().map(|peer| peer.id).collect::<Vec<_>>();

    view! {
        <ul class="peer-list">
            <For each=peer_ids key=|id| *id let:id>
                <PeerRow id=id />
            </For>
        </ul>
    }
}

#[component]
fn PeerRow(id: PeerId) -> impl IntoView {
    let battles = expect_context::<Battles>();
    let info = move || battles.peers().get().into_iter().find(|peer| peer.id == id);
    let name = move || info().map(|peer| peer.name).unwrap_or_default();
    let admin = move || info().is_some_and(|peer| peer.admin);
    let is_host = move || battles.host_id().get() == Some(id);
    let is_self = move || battles.self_id().get() == Some(id);
    let can_administer = move || matches!(battles.role().get(), Role::Host | Role::Admin);
    let show_admin_toggle = move || can_administer() && !is_host() && !is_self();
    let show_kick = move || battles.role().get() == Role::Host && !is_self();
    let toggle_admin = move |_| battles.set_admin(id, !admin());
    let kick = move |_| battles.kick(id);

    // `topic`/`detail` are re-derived on every hover rather than cached: `DetailTip` only reads
    // them at pointerenter/focusin, but a `Signal` costs nothing extra idle and keeps this correct
    // if the seat's own role changes while the tip happens to already be open.
    let role_topic = Signal::derive(move || match (is_host(), admin()) {
        (true, _) => Topic::RoomRoleHost,
        (false, true) => Topic::RoomRoleAdmin,
        (false, false) => Topic::RoomRoleSpectator,
    });
    let role_detail = Signal::derive(move || if is_self() { "This is you.".to_string() } else { String::new() });

    view! {
        <li class="peer-row">
            <DetailTip topic=role_topic detail=role_detail>
                <span class="peer-name">{name}</span>
            </DetailTip>
            {move || is_host().then(|| view! { <span class="peer-badge peer-badge-host">"Host"</span> })}
            {move || admin().then(|| view! {
                <Tip topic=Topic::RoomAdmin>
                    <span class="peer-badge">"Admin"</span>
                </Tip>
            })}
            <span class="peer-actions">
                {move || show_admin_toggle().then(|| view! {
                    <Tip topic=Topic::RoomAdmin>
                        <button class="btn peer-admin-toggle" on:click=toggle_admin>
                            {move || if admin() { "Remove admin" } else { "Make admin" }}
                        </button>
                    </Tip>
                })}
                {move || show_kick().then(|| view! {
                    <Tip topic=Topic::RoomKick>
                        <button class="btn" on:click=kick>"\u{2715}"</button>
                    </Tip>
                })}
            </span>
        </li>
    }
}
