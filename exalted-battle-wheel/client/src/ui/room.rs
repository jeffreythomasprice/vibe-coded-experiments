//! Host or join a server-hosted room: name yourself, name the room, and see who's in it. The
//! server is the one authority on whether a move is legal. Any access code may create a room,
//! join a room, and play; write access inside a room is per connection, controlled by whoever
//! already has it.

use crate::access::Access;
use crate::battle_net::{Battles, Mode};
use crate::prefs::Prefs;
use crate::ui::glossary::Topic;
use crate::ui::{AllRoomsButton, AllRoomsModal, ConfigOpen, DetailTip, Modal, Spinner, Tip};
use leptos::prelude::*;
use leptos::web_sys;
use shared::protocol::{ConnectionId, MAX_NAME_LEN, MAX_ROOM_NAME_LEN};

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
///
/// Host and Join each get their own room-name signal rather than sharing one: `host_room` starts
/// pre-filled with a suggestion (below) so "Host a room" is one click away, and pre-filling would
/// leak a bogus room name into Join if the two branches shared a field.
#[derive(Clone, Copy)]
struct SoloForm {
    menu: RwSignal<MenuChoice>,
    host_room: RwSignal<String>,
    join_room: RwSignal<String>,
    everyone_writes: RwSignal<bool>,
}

impl SoloForm {
    fn new() -> Self {
        Self {
            menu: RwSignal::new(MenuChoice::Root),
            // A plain initializer, not a reactive write -- unlike `Prefs::ensure_player_name`,
            // there's no `Persisted` autosave ordering to worry about here.
            host_room: RwSignal::new(crate::names::random_room_name()),
            join_room: RwSignal::new(String::new()),
            everyone_writes: RwSignal::new(true),
        }
    }
}

/// Whether the multiplayer dialog is open. Provided in `app.rs` (mirrors `ConfigOpen`) so the
/// hamburger menu can toggle it without owning the modal itself.
#[derive(Clone, Copy)]
pub struct RoomOpen(pub RwSignal<bool>);

/// Whether the admin-only "All rooms" browser is open. Provided by `RoomPanelBody` itself, not
/// `app.rs` (unlike `RoomOpen`/`ConfigOpen`) -- it only ever needs to be visible inside the
/// Multiplayer dialog's own subtree, and scoping it there means it's recreated `false` every time
/// that dialog reopens rather than remembering whether it was left open from a previous visit.
#[derive(Clone, Copy)]
pub struct RoomsOpen(pub RwSignal<bool>);

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
    let prefs = expect_context::<Prefs>();
    let form = SoloForm::new();
    provide_context(RoomsOpen(RwSignal::new(false)));

    // Suggests a name before the modal's first paint whenever one isn't saved yet, so the
    // multiplayer form is never blank and "Host"/"Join" are never disabled for want of one. An
    // `Effect` rather than a bare call in the body, to keep the `Persisted` write out of a render
    // pass -- see `Prefs::ensure_player_name`'s doc comment for why the write itself is safe here.
    Effect::new(move |_| {
        prefs.ensure_player_name();
    });

    view! {
        <div class="room-panel">
            {move || match access.me().get() {
                // `startup::run`'s access-code check is still in flight -- not "no code", which
                // would wrongly tell a browser that already has one that it doesn't.
                None if access.busy().get() => view! {
                    <div class="room-pending"><Spinner /> "Checking your access code\u{2026}"</div>
                }.into_any(),
                None => view! { <NoAccessCode close=close /> }.into_any(),
                Some(_) => match battles.mode().get() {
                    Mode::Solo => view! { <SoloMenu form=form /> }.into_any(),
                    Mode::Connecting => view! { <ConnectingView /> }.into_any(),
                    Mode::InRoom => view! { <InRoomView /> }.into_any(),
                },
            }}
            // A sibling of the mode match above, not nested inside `InRoomView`: closing the room
            // an admin is currently in flips `battles.mode()` to `Solo`, and a modal mounted
            // inside `InRoomView` would unmount itself mid-action instead of just going back to
            // showing the (now-shorter) room list.
            <AllRoomsModal />
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
    let SoloForm { menu, host_room, join_room, everyone_writes } = form;

    let host = move |_| {
        let name = prefs.player_name.get();
        let room = host_room.get();
        if name.trim().is_empty() || room.trim().is_empty() {
            return;
        }
        battles.create_room(room.trim().to_string(), name.trim().to_string(), everyone_writes.get());
    };

    let join = move |_| {
        let name = prefs.player_name.get();
        let room = join_room.get();
        if name.trim().is_empty() || room.trim().is_empty() {
            return;
        }
        battles.join_room(room.trim().to_string(), name.trim().to_string());
    };

    let name_ready = move || !prefs.player_name.get().trim().is_empty();
    let host_room_ready = move || !host_room.get().trim().is_empty();
    let join_room_ready = move || !join_room.get().trim().is_empty();
    let back = move |_| menu.set(MenuChoice::Root);

    view! {
        <div class="room-menu">
            <NameField />
            {move || match menu.get() {
                MenuChoice::Root => view! {
                    <div class="room-actions">
                        <button class="btn" on:click=move |_| menu.set(MenuChoice::Hosting)>"Host a room"</button>
                        <button class="btn" on:click=move |_| menu.set(MenuChoice::Joining)>"Join a room"</button>
                        <AllRoomsButton />
                    </div>
                }.into_any(),
                MenuChoice::Hosting => view! {
                    <>
                        <RoomNameField room=host_room randomize=true />
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
                            <button class="btn" on:click=host disabled=move || !name_ready() || !host_room_ready()>
                                "Submit"
                            </button>
                        </div>
                    </>
                }.into_any(),
                MenuChoice::Joining => view! {
                    <>
                        <RoomNameField room=join_room />
                        <div class="room-actions">
                            <button class="btn" on:click=back>"Back"</button>
                            <button class="btn" on:click=join disabled=move || !name_ready() || !join_room_ready()>
                                "Submit"
                            </button>
                        </div>
                    </>
                }.into_any(),
            }}
        </div>
    }
}

/// `randomize` shows a die button that rerolls `room` from `names::random_room_name` -- host-only:
/// on Join, a random name would almost certainly name a room that doesn't exist.
#[component]
fn RoomNameField(room: RwSignal<String>, #[prop(optional)] randomize: bool) -> impl IntoView {
    view! {
        <label class="room-field">
            "Room name"
            <div class="room-field-row">
                <input
                    placeholder="Room name"
                    maxlength=MAX_ROOM_NAME_LEN.to_string()
                    prop:value=move || room.get()
                    on:input=move |ev| room.set(event_target_value(&ev))
                />
                {randomize.then(|| view! {
                    <Tip topic=Topic::RoomRandomName>
                        <button
                            class="btn room-randomize"
                            aria-label="Suggest a random room name"
                            on:click=move |_| room.set(crate::names::random_room_name())
                        >
                            "\u{1F3B2}"
                        </button>
                    </Tip>
                })}
            </div>
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
                <InviteLink />
                <AllRoomsButton />
            </div>
        </div>
    }
}

/// A host-only "Copy invite link" button: builds a link carrying this room's name and an access
/// code a guest can actually use (never an admin's own -- see `Access::invite_code`), copies it to
/// the clipboard, and reveals it in a read-only field as a fallback for when the clipboard write
/// is refused or unavailable.
#[component]
fn InviteLink() -> impl IntoView {
    let access = expect_context::<Access>();
    let battles = expect_context::<Battles>();

    let url: RwSignal<Option<String>> = RwSignal::new(None);
    let copied: RwSignal<Option<String>> = RwSignal::new(None);
    let working: RwSignal<Option<String>> = RwSignal::new(None);

    let click = move |_| {
        if working.get_untracked().is_some() {
            return;
        }
        let Some(room) = battles.room().get_untracked() else { return };
        // Already built (a second click, or a click after the field was revealed): just copy
        // again rather than rebuilding. The clipboard write must be issued synchronously from
        // this handler -- see `copy_to_clipboard`'s own doc comment -- so a cached URL takes this
        // path even though `copy_invite_link` would answer eventually too.
        if let Some(url) = url.get_untracked() {
            copy_to_clipboard(&url, copied, room);
            return;
        }
        copy_invite_link(access, room, url, copied, working);
    };

    // This component only ever has one room to show a link for, so its label just compares
    // `working`/`copied` (which name *some* row's copy last touched -- see `copy_invite_link`'s
    // own doc comment) against that one room's own name.
    let is_this_room = move |named: &RwSignal<Option<String>>| {
        named.get().as_deref() == battles.room().get().as_deref()
    };

    view! {
        {move || battles.is_host().get().then(|| view! {
            <>
                <Tip topic=Topic::RoomInviteLink>
                    <button class="btn" on:click=click disabled=move || is_this_room(&working)>
                        {move || match (is_this_room(&working), is_this_room(&copied)) {
                            (true, _) => "Preparing\u{2026}",
                            (false, true) => "Copied!",
                            (false, false) => "Copy invite link",
                        }}
                    </button>
                </Tip>
                {move || url.get().map(|url| view! {
                    <input
                        class="room-invite-url"
                        readonly=true
                        prop:value=url
                        on:click=move |ev| { event_target::<web_sys::HtmlInputElement>(&ev).select(); }
                    />
                })}
            </>
        })}
    }
}

/// Builds an invite link for `room` (via `Access::invite_code` -- never an admin's own code, see
/// its own doc comment) and copies it to the clipboard, reporting progress into `working`/`url`/
/// `copied`. Shared by the host-only "Copy invite link" button above (one room, a cacheable link)
/// and the admin room browser's per-row link button (`ui::rooms_admin`, many rooms sharing one
/// revealed field, nothing worth caching) -- `working`/`copied` hold *which* room is in that state
/// so more than one caller's button can watch the same pair of signals without stepping on each
/// other's label.
pub(crate) fn copy_invite_link(
    access: Access,
    room: String,
    url: RwSignal<Option<String>>,
    copied: RwSignal<Option<String>>,
    working: RwSignal<Option<String>>,
) {
    if working.get_untracked().is_some() {
        return;
    }
    let Some((origin, path)) = crate::link::origin_and_path() else {
        crate::ui::toast::error("Could not read this page's own address.".to_string());
        return;
    };
    working.set(Some(room.clone()));
    access.invite_code(move |result| {
        working.set(None);
        match result {
            Ok(code) => {
                let built = crate::link::invite_url(&origin, &path, &code, &room);
                url.set(Some(built.clone()));
                // Attempted even though the gesture that started this click may already be gone
                // by now (the admin path just awaited a fetch) -- Safari/Firefox may refuse it,
                // which is exactly what the revealed field is the fallback for. The non-admin
                // path above never has this problem: it never awaits anything, so the gesture is
                // still live.
                copy_to_clipboard(&built, copied, room);
            }
            Err(error) => {
                tracing::error!(%error, "could not prepare an invite link");
                crate::ui::toast::error(format!("Could not prepare an invite link: {error}"));
            }
        }
    });
}

/// Writes to the clipboard and flashes `copied` (naming `room`) for a moment before clearing it
/// back out -- guarded so a slower flash from an earlier click can't clobber a newer one's. Issued
/// synchronously from the click handler that calls it wherever possible: Safari only grants
/// clipboard access while the gesture that started it is still on the call stack, so awaiting
/// anything first can lose it. Silent when there's no clipboard to write to -- an insecure origin
/// (a LAN IP over plain http; a dev server on `127.0.0.1`/`localhost` is still a secure context)
/// has no `navigator.clipboard` at all, and calling into it regardless would throw straight through
/// wasm rather than returning an error; the revealed field is the fallback there.
pub(crate) fn copy_to_clipboard(text: &str, copied: RwSignal<Option<String>>, room: String) {
    let Some(window) = web_sys::window() else { return };
    if !window.is_secure_context() {
        return;
    }
    let promise = window.navigator().clipboard().write_text(text);
    leptos::task::spawn_local_scoped(async move {
        match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(_) => {
                copied.set(Some(room.clone()));
                set_timeout(
                    move || copied.update(|copied| if copied.as_deref() == Some(room.as_str()) { *copied = None }),
                    std::time::Duration::from_millis(1500),
                );
            }
            Err(error) => tracing::warn!(?error, "could not write the invite link to the clipboard"),
        }
    });
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
                maxlength=MAX_NAME_LEN.to_string()
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
    let name = Signal::derive(move || member.get().map(|member| member.name.to_string()).unwrap_or_default());
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
