use leptos::prelude::*;

#[component]
pub(super) fn DiscoverySection() -> impl IntoView {
    view! {
        <div>
            <p class="settings-pane-title">"Discovery"</p>
            <p class="settings-pane-sub">"How newly found services are surfaced."</p>
            <div class="settings-card">
                <div class="settings-card-row">
                    <p style="margin:0; font-size:0.8rem; line-height:1.65; color:var(--color-text-secondary)">
                        "VexBoard automatically discovers running systemd services via D-Bus. \
                         Discovered services appear on the dashboard for you to claim or dismiss."
                    </p>
                </div>
            </div>
        </div>
    }
}
