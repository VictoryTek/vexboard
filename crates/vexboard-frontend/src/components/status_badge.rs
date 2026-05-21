use leptos::prelude::*;

#[component]
pub fn StatusDot(status: String) -> impl IntoView {
    let cls = match status.as_str() {
        "up"   => "status-dot status-dot-up",
        "down" => "status-dot status-dot-down",
        _      => "status-dot status-dot-unknown",
    };

    view! { <span class={cls}></span> }
}
