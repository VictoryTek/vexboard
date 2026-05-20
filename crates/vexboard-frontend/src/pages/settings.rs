use leptos::prelude::*;

#[component]
pub fn SettingsPage() -> impl IntoView {
    view! {
        <div>
            <h1 class="text-xl font-semibold mb-6">"Settings"</h1>

            <div class="space-y-6 max-w-lg">
                // Theme toggle
                <div class="card">
                    <h2 class="text-sm font-medium mb-3">"Appearance"</h2>
                    <div class="flex items-center justify-between">
                        <span class="text-sm text-[var(--color-text-secondary)]">"Dark Mode"</span>
                        <button
                            class="px-3 py-1.5 rounded-lg text-xs bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]"
                            on:click=move |_| {
                                // Toggle theme in localStorage and on <html>
                                #[cfg(target_arch = "wasm32")]
                                {
                                    use wasm_bindgen::JsCast;
                                    let doc = web_sys::window().unwrap().document().unwrap();
                                    let html = doc.document_element().unwrap();
                                    let current = html.class_list().contains("dark");
                                    if current {
                                        html.class_list().remove_1("dark").ok();
                                        html.class_list().add_1("light").ok();
                                    } else {
                                        html.class_list().remove_1("light").ok();
                                        html.class_list().add_1("dark").ok();
                                    }
                                }
                            }
                        >
                            "Toggle"
                        </button>
                    </div>
                </div>

                // Discovery settings
                <div class="card">
                    <h2 class="text-sm font-medium mb-3">"Discovery"</h2>
                    <p class="text-xs text-[var(--color-text-muted)]">
                        "VexBoard automatically discovers running systemd services. \
                         Unclaimed services appear in the discovery panel."
                    </p>
                </div>

                // About
                <div class="card">
                    <h2 class="text-sm font-medium mb-3">"About"</h2>
                    <p class="text-xs text-[var(--color-text-muted)]">
                        "VexBoard v0.1.0 — Self-hosted server dashboard for NixOS and systemd."
                    </p>
                </div>
            </div>
        </div>
    }
}
