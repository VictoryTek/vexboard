use leptos::prelude::*;

use super::ui::{card, row};

#[component]
pub(super) fn AboutSection() -> impl IntoView {
    view! {
        <div>
            <p class="settings-pane-title">"About"</p>
            <p class="settings-pane-sub">"Build details for this VexBoard instance."</p>
            {card("This instance", row(
                "Version",
                "Self-hosted server dashboard for NixOS and systemd.",
                view! {
                    <span style="font-family: ui-monospace, monospace; font-size: 0.8rem; color: var(--color-text-secondary);">
                        {concat!("v", env!("CARGO_PKG_VERSION"))}
                    </span>
                },
            ))}
        </div>
    }
}
