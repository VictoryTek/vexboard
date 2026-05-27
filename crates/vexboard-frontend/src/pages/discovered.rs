use leptos::prelude::*;

use crate::components::discovery_panel::DiscoveryPanel;

#[component]
pub fn DiscoveredPage() -> impl IntoView {
    view! {
        <div>
            <div class="page-header">
                <h1 class="page-title">"Discovered Services"</h1>
            </div>
            <p style="font-size:0.8rem; color:var(--color-text-muted); margin:0 0 1rem;">
                "Services found from Docker/Podman/systemd that are not yet in your dashboard."
            </p>
            <DiscoveryPanel on_added=Callback::new(move |_| {}) />
        </div>
    }
}
