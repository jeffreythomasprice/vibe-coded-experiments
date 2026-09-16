//! The header's hamburger menu: the one entry point to the Multiplayer and Settings dialogs, sitting
//! at the top-right edge of the menu bar in place of what used to be two separate header buttons.

use crate::battle_net::Battles;
use crate::ui::glossary::Topic;
use crate::ui::{room_label, ConfigModal, ConfigOpen, RoomModal, RoomOpen, Tip};
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;

#[component]
pub fn HamburgerMenu() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let room_open = expect_context::<RoomOpen>().0;
    let config_open = expect_context::<ConfigOpen>().0;
    let expanded = RwSignal::new(false);
    let root = NodeRef::<leptos::html::Div>::new();

    let handle = window_event_listener(leptos::ev::mousedown, move |ev: web_sys::MouseEvent| {
        if !expanded.get_untracked() {
            return;
        }
        let clicked_inside = ev
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
            .zip(root.get_untracked())
            .is_some_and(|(target, root)| root.contains(Some(&target)));
        if !clicked_inside {
            expanded.set(false);
        }
    });
    on_cleanup(move || handle.remove());

    view! {
        <div class="hamburger-menu" node_ref=root>
            <button
                class="hamburger-toggle"
                aria-label="Menu"
                aria-expanded=move || expanded.get().to_string()
                on:click=move |_| expanded.update(|open| *open = !*open)
            >
                "\u{2630}"
            </button>
            {move || {
                expanded.get().then(|| view! {
                    <div class="hamburger-dropdown">
                        <Tip topic=Topic::Room>
                            <button
                                class="hamburger-item"
                                on:click=move |_| {
                                    room_open.set(true);
                                    expanded.set(false);
                                }
                            >
                                {move || room_label(battles)}
                            </button>
                        </Tip>
                        <Tip topic=Topic::Config>
                            <button
                                class="hamburger-item"
                                on:click=move |_| {
                                    config_open.set(true);
                                    expanded.set(false);
                                }
                            >
                                "Settings"
                            </button>
                        </Tip>
                    </div>
                })
            }}
        </div>
        <RoomModal />
        <ConfigModal />
    }
}
