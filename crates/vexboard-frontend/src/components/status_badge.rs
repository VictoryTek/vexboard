use leptos::prelude::*;

#[component]
pub fn StatusDot(status: String) -> impl IntoView {
    let (color, animate) = match status.as_str() {
        "up" => ("bg-[var(--color-success)]", true),
        "down" => ("bg-[var(--color-danger)]", true),
        _ => ("bg-[var(--color-text-muted)]", false),
    };

    let classes = if animate {
        format!("w-2 h-2 rounded-full {color} animate-pulse")
    } else {
        format!("w-2 h-2 rounded-full {color}")
    };

    view! {
        <span class={classes}></span>
    }
}
