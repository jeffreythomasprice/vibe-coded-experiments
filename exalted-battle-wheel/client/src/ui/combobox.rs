//! A free-text input that also offers a themed popup of existing values, so a field can both
//! reuse what's already been entered elsewhere and start something new. Deliberately not backed
//! by a native `<datalist>`: that would use the browser's own (unthemed) suggestion box instead
//! of the app's palette and keyboard handling.

use crate::ui::tip;
use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Popup {
    Closed,
    /// Opened by typing: narrowed to what matches the current text.
    Filtered,
    /// Opened by the toggle or an arrow key: every option, regardless of what's typed.
    All,
}

/// Case-insensitive substring match — not a prefix match, so a later word in a multi-word value
/// (e.g. "Legion" in "Tepet Legion") is still found. An empty (or all-whitespace) query matches
/// everything, since that's the "what's already out there?" case.
fn filter_options(options: &[String], typed: &str) -> Vec<String> {
    let needle = typed.trim().to_lowercase();
    if needle.is_empty() {
        return options.to_vec();
    }
    options.iter().filter(|option| option.to_lowercase().contains(&needle)).cloned().collect()
}

/// `value` is free text: typing anything and leaving it as-is is always valid. `options` is
/// offered as a popup underneath — pick one, or ignore it and type something new. Callers wire
/// `options` with `Signal::derive` (or a `Memo`), not a bare closure.
#[component]
pub fn Combobox(
    value: RwSignal<String>,
    #[prop(into)] options: Signal<Vec<String>>,
    /// DOM id for the popup listbox; must be unique on the page.
    list_id: &'static str,
    #[prop(optional)] placeholder: &'static str,
) -> impl IntoView {
    let popup = RwSignal::new(Popup::Closed);
    let highlighted = RwSignal::new(None::<usize>);
    let root = NodeRef::<leptos::html::Div>::new();
    let input = NodeRef::<leptos::html::Input>::new();

    let visible = move || match popup.get() {
        Popup::Closed => Vec::new(),
        Popup::All => options.get(),
        Popup::Filtered => filter_options(&options.get(), &value.get()),
    };

    // Opening a popup here always dismisses whatever teaching tooltip is showing — the tip
    // anchors right below the field, the same spot this popup renders, and would otherwise sit
    // on top of it.
    let open = move |mode: Popup| {
        tip::dismiss();
        popup.set(mode);
    };
    let close = move || {
        popup.set(Popup::Closed);
        highlighted.set(None);
    };
    let commit = move |chosen: String| {
        value.set(chosen);
        close();
    };

    let handle = window_event_listener(leptos::ev::mousedown, move |ev: web_sys::MouseEvent| {
        if popup.get_untracked() == Popup::Closed {
            return;
        }
        let clicked_inside = ev
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
            .zip(root.get_untracked())
            .is_some_and(|(target, root)| root.contains(Some(&target)));
        if !clicked_inside {
            close();
        }
    });
    on_cleanup(move || handle.remove());

    let on_keydown = move |ev: KeyboardEvent| match ev.key().as_str() {
        "ArrowDown" => {
            ev.prevent_default();
            if popup.get_untracked() == Popup::Closed {
                open(Popup::All);
            }
            let count = visible().len();
            if count > 0 {
                highlighted.update(|current| {
                    *current = Some(match *current {
                        Some(i) if i + 1 < count => i + 1,
                        _ => 0,
                    });
                });
            }
        }
        "ArrowUp" => {
            ev.prevent_default();
            if popup.get_untracked() == Popup::Closed {
                open(Popup::All);
            }
            let count = visible().len();
            if count > 0 {
                highlighted.update(|current| {
                    *current = Some(match *current {
                        Some(i) if i > 0 => i - 1,
                        _ => count - 1,
                    });
                });
            }
        }
        "Enter" => {
            if let Some(chosen) = highlighted.get_untracked().and_then(|i| visible().into_iter().nth(i)) {
                ev.prevent_default();
                commit(chosen);
            }
        }
        "Escape" => {
            if popup.get_untracked() != Popup::Closed {
                ev.prevent_default();
                ev.stop_propagation();
                close();
            }
        }
        "Tab" => close(),
        _ => {}
    };

    view! {
        <div class="combobox" node_ref=root>
            <input
                class="combobox-input"
                node_ref=input
                role="combobox"
                aria-autocomplete="list"
                aria-controls=list_id
                aria-expanded=move || if visible().is_empty() { "false" } else { "true" }
                aria-activedescendant=move || highlighted.get().map(|i| format!("{list_id}-option-{i}"))
                autocomplete="off"
                placeholder=placeholder
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                    highlighted.set(None);
                    open(Popup::Filtered);
                }
                on:keydown=on_keydown
            />
            <button
                type="button"
                class="combobox-toggle"
                tabindex="-1"
                aria-label="Show existing values"
                on:mousedown=|ev: MouseEvent| ev.prevent_default()
                on:click=move |_| {
                    // Focus first: if the field wasn't already focused, this fires a focusin
                    // that bubbles up to the wrapping `Tip` and shows the teaching tooltip —
                    // `open`'s dismiss() then needs to run after that, not before, or the
                    // tooltip re-appears on top of the popup it just opened.
                    if let Some(field) = input.get_untracked() {
                        let _ = field.focus();
                    }
                    if popup.get_untracked() == Popup::Closed {
                        open(Popup::All);
                    } else {
                        close();
                    }
                }
            >
                "\u{25BE}"
            </button>
            <ul class="combobox-list" id=list_id role="listbox" hidden=move || visible().is_empty()>
                {move || {
                    visible()
                        .into_iter()
                        .enumerate()
                        .map(|(index, option)| {
                            let chosen = option.clone();
                            view! {
                                <li
                                    class="combobox-option"
                                    class:combobox-option-active=move || highlighted.get() == Some(index)
                                    id=format!("{list_id}-option-{index}")
                                    role="option"
                                    aria-selected=move || {
                                        if highlighted.get() == Some(index) { "true" } else { "false" }
                                    }
                                    on:mousedown=move |ev: MouseEvent| {
                                        ev.prevent_default();
                                        commit(chosen.clone());
                                    }
                                >
                                    {option}
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<String> {
        vec!["Dune People".to_string(), "House Tepet".to_string(), "anathema".to_string()]
    }

    #[test]
    fn an_empty_query_offers_every_option() {
        assert_eq!(filter_options(&options(), ""), options());
        assert_eq!(filter_options(&options(), "   "), options());
    }

    #[test]
    fn filtering_is_case_insensitive() {
        assert_eq!(filter_options(&options(), "ANATHEMA"), vec!["anathema".to_string()]);
    }

    #[test]
    fn filtering_matches_anywhere_not_just_the_start() {
        assert_eq!(filter_options(&options(), "tepet"), vec!["House Tepet".to_string()]);
    }

    #[test]
    fn no_match_yields_no_options() {
        assert!(filter_options(&options(), "zzz").is_empty());
    }
}
