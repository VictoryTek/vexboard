use leptos::prelude::*;

use crate::components::sidebar::{save_sidebar_mode_to_storage, SidebarMode};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let sidebar_mode =
        use_context::<ReadSignal<SidebarMode>>().expect("SidebarMode context must be provided");
    let set_sidebar_mode = use_context::<WriteSignal<SidebarMode>>()
        .expect("set_sidebar_mode context must be provided");

    view! {
        <div>
            <div class="page-header">
                <h1 class="page-title">"Settings"</h1>
            </div>

            <div class="space-y-4" style="max-width: 540px">
                // Appearance
                <div class="card">
                    <h2 class="text-sm font-semibold mb-3"
                        style="color: var(--color-text-primary)">"Appearance"</h2>
                    <div class="flex items-center justify-between gap-4">
                        <div>
                            <p class="text-sm" style="color: var(--color-text-secondary)">"Theme"</p>
                            <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">
                                "Toggle between dark and light mode."
                            </p>
                        </div>
                        <button
                            class="btn-secondary"
                            style="flex-shrink: 0"
                            on:click=move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let doc = web_sys::window().unwrap().document().unwrap();
                                    let html = doc.document_element().unwrap();
                                    let is_dark = html.class_list().contains("dark")
                                        || !html.class_list().contains("light");
                                    if is_dark {
                                        html.class_list().remove_1("dark").ok();
                                        html.class_list().add_1("light").ok();
                                    } else {
                                        html.class_list().remove_1("light").ok();
                                        html.class_list().add_1("dark").ok();
                                    }
                                }
                            }
                        >
                            "Toggle Theme"
                        </button>
                    </div>
                </div>

                // Navigation Sidebar
                <div class="card">
                    <h2 class="text-sm font-semibold mb-3"
                        style="color: var(--color-text-primary)">"Navigation Sidebar"</h2>
                    <div class="space-y-2">
                        {[
                            (SidebarMode::HoverExpand,     "Hover Expand",     "Collapsed by default, expands on hover."),
                            (SidebarMode::AlwaysExpanded,  "Always Expanded",  "Sidebar always shows labels."),
                            (SidebarMode::AlwaysCollapsed, "Always Collapsed", "Sidebar shows icons only."),
                        ].into_iter().map(|(mode, label, desc)| {
                            let mode_for_class = mode.clone();
                            let mode_for_click = mode.clone();
                            view! {
                                <button
                                    class=move || if sidebar_mode.get() == mode_for_class { "nav-item-active" } else { "nav-item" }
                                    style="width: 100%; text-align: left; padding: 0.625rem 0.75rem;"
                                    on:click=move |_| {
                                        let m = mode_for_click.clone();
                                        save_sidebar_mode_to_storage(&m);
                                        set_sidebar_mode.set(m);
                                    }
                                >
                                    <div>
                                        <p class="text-sm font-medium">{label}</p>
                                        <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">{desc}</p>
                                    </div>
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                // Discovery
                <div class="card">
                    <h2 class="text-sm font-semibold mb-2"
                        style="color: var(--color-text-primary)">"Service Discovery"</h2>
                    <p class="text-xs leading-relaxed"
                       style="color: var(--color-text-muted)">
                        "VexBoard automatically discovers running systemd services via D-Bus. \
                         Discovered services appear in the dashboard for you to claim or dismiss."
                    </p>
                </div>

                // About
                <div class="card">
                    <h2 class="text-sm font-semibold mb-2"
                        style="color: var(--color-text-primary)">"About"</h2>
                    <p class="text-xs" style="color: var(--color-text-muted)">
                        "VexBoard v0.1.0 — Self-hosted server dashboard for NixOS and systemd."
                    </p>
                </div>
            </div>
        </div>
    }
}
