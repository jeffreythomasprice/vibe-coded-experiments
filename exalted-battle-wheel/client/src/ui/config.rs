//! The gear-icon settings dialog: check or set the browser's own access code, and -- if that code
//! is an admin's -- manage every access code the server knows about. The one place in the app that
//! exercises the server's access-code HTTP API end to end.

use crate::access::Access;
use crate::ui::glossary::Topic;
use crate::ui::{Modal, Spinner, Tip};
use leptos::prelude::*;
use shared::access::AccessCode;
use time::OffsetDateTime;

/// "YYYY-MM-DD HH:MM UTC" -- shared with `ui::rooms_admin`'s "Last active" column, so a timestamp
/// reads the same way in both admin tables.
pub(crate) fn format_timestamp(at: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// Which code (if any) is pending a delete confirmation -- dialog-local navigation state, not part
/// of `Access` itself.
#[derive(Clone, Copy)]
struct ConfirmingDelete(RwSignal<Option<String>>);

/// Whether the settings dialog is open. Provided in `app.rs` (not owned locally by
/// `ConfigModal`, unlike most of this app's modals) so `ui::room`'s "no access code" prompt and the
/// hamburger menu can open it directly instead of just telling the user where to find it.
#[derive(Clone, Copy)]
pub struct ConfigOpen(pub RwSignal<bool>);

#[component]
pub fn ConfigModal() -> impl IntoView {
    let open = expect_context::<ConfigOpen>().0;

    view! {
        {move || {
            open.get().then(|| view! {
                <Modal title="Access code" wide=true on_close=move || open.set(false)>
                    <ConfigPanel />
                </Modal>
            })
        }}
    }
}

#[component]
fn ConfigPanel() -> impl IntoView {
    let access = expect_context::<Access>();
    let busy = access.busy();
    provide_context(ConfirmingDelete(RwSignal::new(None)));

    view! {
        <div class="access-panel" class:access-busy=move || busy.get()>
            {move || {
                busy.get().then(|| view! {
                    <div class="access-pending"><Spinner /> "Working\u{2026}"</div>
                })
            }}
            {move || {
                match access.me().get() {
                    Some(code) => view! { <IdentityPanel code=code /> }.into_any(),
                    None => view! { <SignInForm /> }.into_any(),
                }
            }}
        </div>
        <DeleteConfirmation />
    }
}

#[component]
fn SignInForm() -> impl IntoView {
    let access = expect_context::<Access>();
    let busy = access.busy();
    let input = RwSignal::new(String::new());

    let submit = move || {
        let token = input.get_untracked().trim().to_string();
        if !token.is_empty() {
            access.sign_in(token);
        }
    };

    view! {
        <label class="room-field">
            <Tip topic=Topic::ConfigCode><span>"Access code"</span></Tip>
            <input
                class="access-code-input"
                placeholder="Paste an access code"
                prop:value=move || input.get()
                on:input=move |ev| input.set(event_target_value(&ev))
                on:keydown=move |ev| if ev.key() == "Enter" { submit() }
                disabled=move || busy.get()
            />
        </label>
        {move || access.error().get().map(|error| view! { <p class="room-error">{error}</p> })}
        <button class="btn" on:click=move |_| submit() disabled=move || busy.get() || input.get().trim().is_empty()>
            "Connect"
        </button>
    }
}

#[component]
fn IdentityPanel(code: AccessCode) -> impl IntoView {
    let access = expect_context::<Access>();
    let busy = access.busy();
    let is_admin = code.is_admin;

    view! {
        <dl class="access-meta">
            <dt>"Access code"</dt>
            <dd class="access-key">{code.access_key.to_string()}</dd>
            <dt>"Admin"</dt>
            <dd>{if code.is_admin { "Yes" } else { "No" }}</dd>
            <dt>"Created"</dt>
            <dd>{format_timestamp(*code.created_at)}</dd>
        </dl>
        <button class="btn" on:click=move |_| access.clear() disabled=move || busy.get()>
            "Clear"
        </button>
        {is_admin.then(|| view! { <AdminPanel my_key=code.access_key.to_string() /> })}
    }
}

#[component]
fn AdminPanel(my_key: String) -> impl IntoView {
    let access = expect_context::<Access>();

    view! {
        <div class="access-admin">
            <Tip topic=Topic::ConfigTable><h3 class="access-table-heading">"All access codes"</h3></Tip>
            <div class="access-table-scroll">
                <table class="access-table">
                    <thead>
                        <tr>
                            <th>"Code"</th>
                            <th><Tip topic=Topic::ConfigAdminFlag><span>"Admin"</span></Tip></th>
                            <th>"Created"</th>
                            <th><Tip topic=Topic::ConfigDelete><span>"Delete"</span></Tip></th>
                        </tr>
                    </thead>
                    <tbody>
                        <For each=move || access.codes().get() key=|code| code.access_key.to_string() let:code>
                            <CodeRow code=code my_key=my_key.clone() />
                        </For>
                    </tbody>
                </table>
            </div>
            <CreateForm />
        </div>
    }
}

#[component]
fn CodeRow(code: AccessCode, my_key: String) -> impl IntoView {
    let access = expect_context::<Access>();
    let confirming = expect_context::<ConfirmingDelete>().0;
    let busy = access.busy();
    let is_self = code.access_key.to_string() == my_key;
    let access_key = code.access_key.to_string();

    let toggle_admin = move |ev| access.set_admin(access_key.clone(), event_target_checked(&ev));

    view! {
        <tr>
            <td class="access-key">
                {code.access_key.to_string()}
                {is_self.then(|| view! {
                    <Tip topic=Topic::ConfigSelf><span class="peer-badge">"You"</span></Tip>
                })}
            </td>
            <td>
                <input
                    type="checkbox"
                    prop:checked=code.is_admin
                    on:change=toggle_admin
                    disabled=move || busy.get() || is_self
                />
            </td>
            <td class="access-created">{format_timestamp(*code.created_at)}</td>
            <td>
                {(!is_self).then({
                    let access_key = code.access_key.to_string();
                    move || view! {
                        <button
                            class="btn access-delete"
                            disabled=move || busy.get()
                            on:click=move |_| confirming.set(Some(access_key.clone()))
                        >
                            "\u{2715}"
                        </button>
                    }
                })}
            </td>
        </tr>
    }
}

#[component]
fn DeleteConfirmation() -> impl IntoView {
    let access = expect_context::<Access>();
    let confirming = expect_context::<ConfirmingDelete>().0;
    let busy = access.busy();

    view! {
        {move || {
            confirming.get().map(|access_key| {
                let confirm_key = access_key.clone();
                view! {
                    <Modal title="Delete access code?" on_close=move || confirming.set(None)>
                        <p class="reset-warning">
                            "This immediately revokes \"" {access_key.clone()}
                            "\". Anyone using it will lose access."
                        </p>
                        <div class="reset-actions">
                            <button class="btn" on:click=move |_| confirming.set(None)>"Cancel"</button>
                            <button
                                class="btn reset-confirm"
                                disabled=move || busy.get()
                                on:click=move |_| {
                                    access.delete(confirm_key.clone());
                                    confirming.set(None);
                                }
                            >
                                "Delete"
                            </button>
                        </div>
                    </Modal>
                }
            })
        }}
    }
}

#[component]
fn CreateForm() -> impl IntoView {
    let access = expect_context::<Access>();
    let busy = access.busy();
    let input = RwSignal::new(String::new());
    let is_admin = RwSignal::new(false);

    let submit = move |_| {
        let key = input.get_untracked().trim().to_string();
        access.create(if key.is_empty() { None } else { Some(key) }, is_admin.get_untracked());
        input.set(String::new());
        is_admin.set(false);
    };

    view! {
        <div class="access-create">
            <Tip topic=Topic::ConfigCreate>
                <label class="room-field">
                    "New access code (blank to generate one)"
                    <input
                        class="access-code-input"
                        prop:value=move || input.get()
                        on:input=move |ev| input.set(event_target_value(&ev))
                        disabled=move || busy.get()
                    />
                </label>
            </Tip>
            <label class="room-field-inline">
                <input
                    type="checkbox"
                    prop:checked=move || is_admin.get()
                    on:change=move |ev| is_admin.set(event_target_checked(&ev))
                    disabled=move || busy.get()
                />
                "Admin"
            </label>
            <button class="btn" on:click=submit disabled=move || busy.get()>
                "Create"
            </button>
        </div>
    }
}
