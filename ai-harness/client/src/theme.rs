//! `use_theme`: the whole theme document, as context.
//!
//! Modeled on `leptos-use`'s `use_color_mode`, but the context value is a
//! *palette*, not a light/dark flag — so a future user-authored document
//! drops straight in without any consumer changing. The document is applied
//! by writing its CSS custom properties as inline declarations on `<html>`,
//! which outrank every rule in `style.css`; that stylesheet's
//! `@media (prefers-color-scheme: dark)` block stays as the pre-boot
//! fallback for the brief window before this runs. See [`apply`] for why that
//! combination is safe.
//!
//! Owns exactly one piece of global mutable state — the root element's inline
//! style — and one effect writes it. Nothing else in the crate should touch it.

use leptos::prelude::*;
use shared::theme::{resolve, Theme, ThemeChoice, UserTheme, THEME_PREFERENCE_KEY};
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;

use crate::commands;

const DARK_QUERY: &str = "(prefers-color-scheme: dark)";

/// The theme, as every view sees it.
///
/// `theme` is a [`Memo`], not an `RwSignal`: the document is *derived* from
/// `choice` and `system_dark`, and nothing outside this module has any
/// business setting it directly. Use [`ThemeContext::set_choice`] to change
/// themes.
#[derive(Debug, Clone, Copy)]
pub struct ThemeContext {
    /// The palette currently painted onto `<html>`.
    pub theme: Memo<Theme>,
    /// What the user picked. `System` tracks the OS.
    pub choice: RwSignal<ThemeChoice>,
    /// Live `prefers-color-scheme: dark`. `false` if this webview has no
    /// `matchMedia` at all.
    pub system_dark: RwSignal<bool>,
    /// The user's own theme catalog — every row in the `themes` table.
    /// `theme` resolves a `ThemeChoice::User` against this, so an edit or
    /// delete just needs to reload this list; nothing else about the theme
    /// pipeline changes.
    pub user_themes: RwSignal<Vec<UserTheme>>,
    /// The last failure to *persist* a choice. A failed write never blocks
    /// the switch — see [`ThemeContext::set_choice`].
    pub error: RwSignal<Option<String>>,
}

impl ThemeContext {
    /// Switch themes now; persist in the background.
    ///
    /// Optimistic on purpose: the paint is driven off `choice`, so the UI
    /// never waits on SQLite for something this cheap. A failed write leaves
    /// the in-memory choice as the user set it and reports itself through
    /// `error`; the next start falls back to whatever is still stored, which
    /// is the honest outcome.
    pub fn set_choice(self, choice: ThemeChoice) {
        self.choice.set(choice);
        self.error.set(None);
        spawn_local(async move {
            match commands::set_preference(THEME_PREFERENCE_KEY, &choice.to_stored()).await {
                Ok(()) => {}
                Err(err) => {
                    tracing::warn!(message = %err.message, "could not save the theme choice");
                    self.error.set(Some(err.message));
                }
            }
        });
    }

    /// Refetch the user's theme catalog. Called once at startup and again
    /// after every create/update/delete, so a saved edit — including one to
    /// the currently active theme — repaints without any extra wiring: the
    /// `theme` `Memo` re-resolves as soon as `user_themes` changes.
    pub fn reload_user_themes(self) {
        spawn_local(async move {
            match commands::list_themes().await {
                Ok(themes) => self.user_themes.set(themes),
                Err(err) => {
                    tracing::warn!(message = %err.message, "could not load user themes");
                }
            }
        });
    }
}

/// Install the theme context and start painting the root element. Call once,
/// from `App` — `provide_context` is a silent no-op without a reactive owner,
/// so calling this from `main` before `mount_to_body` would leave every
/// [`use_theme`] call panicking.
pub fn provide_theme() {
    // `Result` (no `matchMedia` at all) and `Option` (a query the engine
    // can't parse) both collapse to "assume light" — the same answer the
    // stylesheet's bare `:root` gives, so a webview without media queries
    // degrades to exactly the pre-boot appearance instead of to something new.
    let media = window().match_media(DARK_QUERY).ok().flatten();
    let dark_now = media.as_ref().is_some_and(|query| query.matches());

    let choice = RwSignal::new(ThemeChoice::System);
    let system_dark = RwSignal::new(dark_now);
    let user_themes = RwSignal::new(Vec::<UserTheme>::new());
    let theme = Memo::new(move |_| resolve(choice.get(), system_dark.get(), &user_themes.read()));
    let error = RwSignal::new(None::<String>);

    let ctx = ThemeContext { theme, choice, system_dark, user_themes, error };
    provide_context(ctx);

    // The one writer of the root element's inline style. Re-runs whenever the
    // choice changes or the OS flips; since `theme` is a `Memo`, an OS flip
    // while an explicit Light/Dark is selected recomputes to an equal
    // document and never touches the DOM.
    Effect::new(move |_| {
        ctx.theme.with(apply);
    });

    subscribe_to_os_changes(media, system_dark);

    // The user's theme catalog, read once — same "nothing to track, runs
    // exactly once" `Effect::new` + `spawn_local` idiom as the stored-choice
    // read just below. Read before the stored choice resolves so a `User`
    // choice has something to resolve against as soon as it's set, not one
    // tick later.
    Effect::new(move |_| ctx.reload_user_themes());

    // The stored choice, read once. `Effect::new` + `spawn_local` rather than
    // a bare `spawn_local` to match how every other fetch in this crate is
    // written; there is nothing to track here, so it runs exactly once.
    Effect::new(move |_| {
        spawn_local(async move {
            match commands::get_preference(THEME_PREFERENCE_KEY).await {
                Ok(stored) => choice.set(ThemeChoice::from_stored(stored.as_deref())),
                Err(err) => {
                    // Not surfaced in `error`: nothing in the UI is wrong
                    // yet, and "System" is a defensible thing to be showing.
                    tracing::warn!(
                        message = %err.message,
                        "could not read the stored theme; staying on System"
                    );
                }
            }
        });
    });
}

/// The theme context. Panics if `App` did not call [`provide_theme`].
pub fn use_theme() -> ThemeContext {
    use_context::<ThemeContext>().expect("ThemeContext is provided by App")
}

/// Paint `theme` onto the document element as inline declarations.
///
/// Inline declarations outrank *every* rule in `style.css` — CSS sorts style
/// attribute above specificity — so the stylesheet's
/// `@media (prefers-color-scheme: dark)` block can stay exactly as it is and
/// serve as the pre-wasm-boot fallback without ever fighting an explicit
/// Light choice on a dark OS.
///
/// `color-scheme` goes on as a real property, not a custom one: it is what
/// tells the platform to draw its own scrollbars, text caret, and native form
/// controls light or dark. Without it, Light-on-a-dark-OS is light content in
/// dark chrome.
///
/// Every write is fallible and every failure is logged rather than
/// propagated: a rejected `setProperty` means one wrong color, and blowing up
/// the effect would leave the *rest* of the palette half-applied.
fn apply(theme: &Theme) {
    let Some(root) = document().document_element() else {
        tracing::error!("no document element; theme not applied");
        return;
    };
    // `<html>` is always an `HTMLHtmlElement`, but a failed cast is still
    // reported rather than unwrapped — this runs on every theme change.
    let root = match root.dyn_into::<web_sys::HtmlElement>() {
        Ok(root) => root,
        Err(_) => {
            tracing::error!("the document element is not an HtmlElement; theme not applied");
            return;
        }
    };
    let style = root.style();

    for (name, value) in theme.css_vars() {
        if let Err(err) = style.set_property(name, value) {
            tracing::warn!(name, ?err, "could not set a theme custom property");
        }
    }
    let scheme = if theme.is_dark() { "dark" } else { "light" };
    if let Err(err) = style.set_property("color-scheme", scheme) {
        tracing::warn!(?err, "could not set color-scheme");
    }
}

/// Keep `system_dark` in step with the OS.
///
/// The listener reads back off the `MediaQueryList` rather than off the
/// event, which is what lets this skip web-sys' `MediaQueryListEvent`
/// feature entirely — the event carries nothing the list doesn't already
/// have.
///
/// Both the closure and the list are leaked, deliberately:
///
/// * The closure must outlive every future OS flip, and `on_cleanup` will not
///   take it (it wants `Send + Sync`, which a `Closure` is not).
/// * The list itself is leaked because an unreferenced `MediaQueryList` has
///   historically stopped delivering `change` in some WebKit versions — which
///   is what Tauri runs on Linux.
///
/// One bounded leak, once, for the life of the process; the app root is never
/// unmounted.
fn subscribe_to_os_changes(media: Option<web_sys::MediaQueryList>, system_dark: RwSignal<bool>) {
    let Some(media) = media else {
        tracing::warn!("this webview has no matchMedia; System will not follow the OS");
        return;
    };

    let probe = media.clone();
    let on_change = Closure::wrap(Box::new(move |_event: JsValue| {
        system_dark.set(probe.matches());
    }) as Box<dyn FnMut(JsValue)>);

    if let Err(err) =
        media.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
    {
        tracing::warn!(?err, "could not subscribe to prefers-color-scheme");
    }

    on_change.forget();
    std::mem::forget(media);
}
