//! Host or join a server-hosted room: name yourself, name the room, and see who's in it. Replaces
//! the old peer-to-peer version — there's a real server now, so no connection codes to exchange by
//! hand, no NAT to fight, and the server (not a two-phase-commit vote) is the one authority on
//! whether a move is legal. Any access code may create a room, join a room, and play; write access
//! inside a room is per connection, controlled by whoever already has it.

use crate::access::Access;
use crate::battle_net::{Battles, Mode};
use crate::prefs::Prefs;
use crate::ui::glossary::Topic;
use crate::ui::{ConfigOpen, DetailTip, Modal, Spinner, Tip};
use leptos::prelude::*;
use shared::protocol::ConnectionId;

/// Which sub-view `Mode::Solo` is showing. Unlike the room's actual state (owned by `Battles`),
/// this is purely local UI navigation with nothing to keep in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuChoice {
    Root,
    Hosting,
    Joining,
}

/// The Solo menu's in-progress form, owned by `RoomPanelBody` rather than `SoloMenu` itself: the
/// `match battles.mode()` in `RoomPanelBody` rebuilds `SoloMenu` from scratch on every mode
/// change, so a join that fails (Solo -> Connecting -> Solo) would otherwise throw away both the
/// branch the user picked and the room name they'd typed.
#[derive(Clone, Copy)]
struct SoloForm {
    menu: RwSignal<MenuChoice>,
    room: RwSignal<String>,
    everyone_writes: RwSignal<bool>,
}

impl SoloForm {
    fn new() -> Self {
        Self { menu: RwSignal::new(MenuChoice::Root), room: RwSignal::new(String::new()), everyone_writes: RwSignal::new(true) }
    }
}

/// Whether the multiplayer dialog is open. Provided in `app.rs` (mirrors `ConfigOpen`) so the
/// hamburger menu can toggle it without owning the modal itself.
#[derive(Clone, Copy)]
pub struct RoomOpen(pub RwSignal<bool>);

/// The hamburger menu's label for the Multiplayer item, reflecting the current connection state.
pub fn room_label(battles: Battles) -> String {
    match battles.mode().get() {
        Mode::Solo => "Multiplayer (Solo)".to_string(),
        Mode::Connecting => "Multiplayer (Connecting\u{2026})".to_string(),
        Mode::InRoom => {
            let room = battles.room().get().unwrap_or_default();
            format!("Multiplayer ({room} \u{2014} {})", battles.members().get().len())
        }
    }
}

#[component]
pub fn RoomModal() -> impl IntoView {
    let open = expect_context::<RoomOpen>().0;

    view! {
        {move || {
            open.get().then(|| view! {
                <Modal title="Multiplayer" on_close=move || open.set(false)>
                    <RoomPanelBody close=move || open.set(false) />
                </Modal>
            })
        }}
    }
}

/// A header-level shortcut to the same `RoomModal` the hamburger menu opens, shown only once
/// there's a room to show — the hamburger's own "Multiplayer" item stays the way to get there from
/// Solo or Connecting.
#[component]
pub fn RoomStatusButton() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let open = expect_context::<RoomOpen>().0;

    view! {
        {move || {
            (battles.mode().get() == Mode::InRoom).then(|| view! {
                <Tip topic=Topic::Room>
                    <button
                        class="room-status-button"
                        aria-label=move || room_label(battles)
                        on:click=move |_| open.set(true)
                    >
                        "\u{1FAC2}"
                    </button>
                </Tip>
            })
        }}
    }
}

#[component]
fn RoomPanelBody(close: impl Fn() + Copy + Send + 'static) -> impl IntoView {
    let access = expect_context::<Access>();
    let battles = expect_context::<Battles>();
    let form = SoloForm::new();

    view! {
        <div class="room-panel">
            {move || match access.me().get() {
                None => view! { <NoAccessCode close=close /> }.into_any(),
                Some(_) => match battles.mode().get() {
                    Mode::Solo => view! { <SoloMenu form=form /> }.into_any(),
                    Mode::Connecting => view! { <ConnectingView /> }.into_any(),
                    Mode::InRoom => view! { <InRoomView /> }.into_any(),
                },
            }}
        </div>
    }
}

#[component]
fn NoAccessCode(close: impl Fn() + Copy + Send + 'static) -> impl IntoView {
    let config_open = expect_context::<ConfigOpen>().0;

    view! {
        <div class="room-menu">
            <p>"Multiplayer needs an access code. Set one up, then come back here to host or join a room."</p>
            <button
                class="btn"
                on:click=move |_| {
                    close();
                    config_open.set(true);
                }
            >
                "Set up an access code"
            </button>
        </div>
    }
}

#[component]
fn SoloMenu(form: SoloForm) -> impl IntoView {
    let battles = expect_context::<Battles>();
    let prefs = expect_context::<Prefs>();
    let SoloForm { menu, room: room_input, everyone_writes } = form;

    let host = move |_| {
        let name = prefs.player_name.get();
        let room = room_input.get();
        if name.trim().is_empty() || room.trim().is_empty() {
            return;
        }
        battles.create_room(room.trim().to_string(), name.trim().to_string(), everyone_writes.get());
    };

    let join = move |_| {
        let name = prefs.player_name.get();
        let room = room_input.get();
        if name.trim().is_empty() || room.trim().is_empty() {
            return;
        }
        battles.join_room(room.trim().to_string(), name.trim().to_string());
    };

    let name_ready = move || !prefs.player_name.get().trim().is_empty();
    let room_ready = move || !room_input.get().trim().is_empty();
    let back = move |_| menu.set(MenuChoice::Root);

    view! {
        <div class="room-menu">
            <NameField />
            {move || match menu.get() {
                MenuChoice::Root => view! {
                    <div class="room-actions">
                        <button class="btn" on:click=move |_| menu.set(MenuChoice::Hosting)>"Host a room"</button>
                        <button class="btn" on:click=move |_| menu.set(MenuChoice::Joining)>"Join a room"</button>
                    </div>
                }.into_any(),
                MenuChoice::Hosting => view! {
                    <>
                        <RoomNameField room=room_input />
                        <Tip topic=Topic::RoomEveryoneWrites>
                            <label class="room-field-inline">
                                <input
                                    type="checkbox"
                                    prop:checked=move || everyone_writes.get()
                                    on:change=move |ev| everyone_writes.set(event_target_checked(&ev))
                                />
                                "Everyone who joins can edit"
                            </label>
                        </Tip>
                        <div class="room-actions">
                            <button class="btn" on:click=back>"Back"</button>
                            <button class="btn" on:click=host disabled=move || !name_ready() || !room_ready()>
                                "Submit"
                            </button>
                        </div>
                    </>
                }.into_any(),
                MenuChoice::Joining => view! {
                    <>
                        <RoomNameField room=room_input />
                        <div class="room-actions">
                            <button class="btn" on:click=back>"Back"</button>
                            <button class="btn" on:click=join disabled=move || !name_ready() || !room_ready()>
                                "Submit"
                            </button>
                        </div>
                    </>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn RoomNameField(room: RwSignal<String>) -> impl IntoView {
    view! {
        <label class="room-field">
            "Room name"
            <input
                placeholder="Room name"
                prop:value=move || room.get()
                on:input=move |ev| room.set(event_target_value(&ev))
            />
        </label>
    }
}

#[component]
fn ConnectingView() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let leave = move |_| battles.leave();

    view! {
        <div class="room-menu">
            <div class="room-pending">
                <Spinner />
                {move || match battles.room().get() {
                    Some(room) => format!("Reconnecting to \u{201c}{room}\u{201d}\u{2026}"),
                    None => "Connecting\u{2026}".to_string(),
                }}
            </div>
            <button class="btn" on:click=leave>"Cancel"</button>
        </div>
    }
}

#[component]
fn InRoomView() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let leave = move |_| battles.leave();
    let refresh = move |_| battles.resync();

    view! {
        <div class="room-menu">
            <NameField />
            <h3 class="room-name-heading">{move || battles.room().get().unwrap_or_default()}</h3>
            <MemberList />
            <EveryoneWritesToggle />
            <div class="room-actions">
                <button class="btn" on:click=refresh>"Refresh"</button>
                <button class="btn" on:click=leave>"Leave room"</button>
            </div>
        </div>
    }
}

#[component]
fn EveryoneWritesToggle() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let can_administer = move || !battles.read_only().get();

    view! {
        {move || can_administer().then(|| view! {
            <Tip topic=Topic::RoomEveryoneWrites>
                <label class="room-field-inline">
                    <input
                        type="checkbox"
                        prop:checked=move || battles.everyone_writes().get()
                        on:change=move |ev| battles.set_everyone_writes(event_target_checked(&ev))
                    />
                    "Everyone who joins can edit"
                </label>
            </Tip>
        })}
    }
}

/// The same field whether Solo (writes straight to `prefs.player_name` for the next host/join) or
/// InRoom (also asks the room to rename this connection). `prefs.player_name` follows every
/// keystroke; the room only hears about a rename on `on:change` (blur or Enter), since every
/// accepted one costs a `Members` rebroadcast to everyone else, not worth paying per keystroke.
#[component]
fn NameField() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let prefs = expect_context::<Prefs>();

    let on_change = move |ev| {
        let name = event_target_value(&ev);
        if battles.mode().get_untracked() == Mode::InRoom {
            battles.rename(name);
        }
    };

    view! {
        <label class="room-field">
            <Tip topic=Topic::RoomRename><span>"Your name"</span></Tip>
            <input
                placeholder="Name"
                prop:value=move || prefs.player_name.get()
                on:input=move |ev| prefs.player_name.set(event_target_value(&ev))
                on:change=on_change
            />
        </label>
    }
}

#[component]
fn MemberList() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let member_ids = move || battles.members().get().iter().map(|member| member.id.clone()).collect::<Vec<_>>();

    view! {
        <ul class="peer-list">
            <For each=member_ids key=|id| id.clone() let:id>
                <MemberRow id=id />
            </For>
        </ul>
    }
}

#[component]
fn MemberRow(id: ConnectionId) -> impl IntoView {
    let battles = expect_context::<Battles>();

    // `Signal<T>` is `Copy` (a handle into the reactive graph, not a closure carrying its own
    // captured environment), so every one of these can be captured freely by as many `view!`
    // closures below as need it — unlike a raw `move ||` closure over a non-`Copy` `id`, which
    // only one of them could ever take ownership of.
    let member = {
        let id = id.clone();
        Signal::derive(move || battles.members().get().into_iter().find(|member| member.id == id))
    };
    let name = Signal::derive(move || member.get().map(|member| member.name).unwrap_or_default());
    let can_write = Signal::derive(move || member.get().is_some_and(|member| member.can_write));
    let is_host = Signal::derive(move || member.get().is_some_and(|member| member.is_host));
    let is_self = {
        let id = id.clone();
        Signal::derive(move || battles.self_id().get().as_ref() == Some(&id))
    };
    // Mirrors the server's own `require_can_administer` (`server/src/ws/handler.rs`): the actor
    // must currently be able to write, and neither of these controls ever targets the actor
    // themselves or the room's host.
    let can_administer = Signal::derive(move || !battles.read_only().get() && !is_self.get() && !is_host.get());

    let role_topic = Signal::derive(move || match (is_host.get(), can_write.get()) {
        (true, _) => Topic::RoomRoleHost,
        (false, true) => Topic::RoomRoleWriter,
        (false, false) => Topic::RoomRoleReader,
    });
    let role_detail = Signal::derive(move || if is_self.get() { "This is you.".to_string() } else { String::new() });

    // Each owns its own independent clone of `id`, made once here rather than at every place the
    // button below re-renders — unlike the `Signal`s above, a plain closure over a non-`Copy` id
    // can't just be re-derived cheaply, so this is the one clone it needs for its whole lifetime.
    let toggle_write = {
        let id = id.clone();
        move |_| battles.set_writable(id.clone(), !can_write.get_untracked())
    };
    let kick = move |_| battles.kick(id.clone());

    view! {
        <li class="peer-row">
            <DetailTip topic=role_topic detail=role_detail>
                <span class="peer-name">{move || name.get()}</span>
            </DetailTip>
            {move || is_host.get().then(|| view! { <span class="peer-badge peer-badge-host">"Host"</span> })}
            {move || can_write.get().then(|| view! {
                <Tip topic=Topic::RoomCanWrite>
                    <span class="peer-badge">"Can edit"</span>
                </Tip>
            })}
            <span class="peer-actions">
                {move || {
                    let toggle_write = toggle_write.clone();
                    can_administer.get().then(move || view! {
                        <Tip topic=Topic::RoomCanWrite>
                            <button class="btn peer-admin-toggle" on:click=toggle_write>
                                {move || if can_write.get() { "Make read-only" } else { "Allow edits" }}
                            </button>
                        </Tip>
                    })
                }}
                {move || {
                    let kick = kick.clone();
                    can_administer.get().then(move || view! {
                        <Tip topic=Topic::RoomKick>
                            <button class="btn" on:click=kick>"\u{2715}"</button>
                        </Tip>
                    })
                }}
            </span>
        </li>
    }
}
