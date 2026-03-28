use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[component]
pub fn AuthGuard(children: Children) -> impl IntoView {
    if !crate::auth::is_authenticated() {
        let navigate = use_navigate();
        navigate("/login", Default::default());
        return view! { <p>"Redirecting..."</p> }.into_any();
    }

    children().into_any()
}
