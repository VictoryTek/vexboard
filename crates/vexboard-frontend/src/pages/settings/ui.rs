use leptos::prelude::*;

/// A titled card wrapping a set of settings rows.
pub(super) fn card(title: &'static str, children: impl IntoView) -> impl IntoView {
    view! {
        <div class="settings-card">
            <div class="settings-card-head">{title}</div>
            {children}
        </div>
    }
}

/// A label/hint/control row inside a card, control placed beside the text.
pub(super) fn row(
    label: &'static str,
    hint: &'static str,
    control: impl IntoView,
) -> impl IntoView {
    view! {
        <div class="settings-card-row">
            <div class="settings-card-row-txt">
                <p class="settings-card-row-label">{label}</p>
                <p class="settings-card-row-hint">{hint}</p>
            </div>
            <div class="settings-card-row-ctl">{control}</div>
        </div>
    }
}

/// Like `row`, but stacks the control full-width below the label instead of
/// beside it — used when the control needs more horizontal room than a
/// side-by-side layout allows (e.g. a row of option buttons).
pub(super) fn row_stack(
    label: &'static str,
    hint: &'static str,
    control: impl IntoView,
) -> impl IntoView {
    view! {
        <div class="settings-card-row settings-card-row-stack">
            <div class="settings-card-row-txt">
                <p class="settings-card-row-label">{label}</p>
                <p class="settings-card-row-hint">{hint}</p>
            </div>
            <div>{control}</div>
        </div>
    }
}
