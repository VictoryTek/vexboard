use leptos::prelude::*;

#[component]
pub fn SettingsPage() -> impl IntoView {
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
                                    use wasm_bindgen::JsCast;
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
