#![allow(dead_code)]
use leptos::prelude::*;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EditFormData {
    pub display_name: String,
    pub description: String,
    pub url: String,
    pub icon: String,
    pub group_id: Option<i64>,
    pub probe_enabled: bool,
    pub probe_interval: i64,
}

#[allow(dead_code)]
#[component]
pub fn EditModal(
    #[prop(into)] visible: Signal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(optional)] initial: Option<EditFormData>,
) -> impl IntoView {
    let initial = initial.unwrap_or(EditFormData {
        display_name: String::new(),
        description: String::new(),
        url: String::new(),
        icon: String::new(),
        group_id: None,
        probe_enabled: true,
        probe_interval: 30,
    });

    let (name, set_name) = signal(initial.display_name);
    let (desc, set_desc) = signal(initial.description);
    let (url, set_url) = signal(initial.url);
    let (icon, set_icon) = signal(initial.icon);

    view! {
        <Show when=move || visible.get()>
            <div class="fixed inset-0 z-50 flex items-center justify-center">
                // Backdrop
                <div
                    class="absolute inset-0 bg-black/60 backdrop-blur-sm"
                    on:click=move |_| on_close.run(())
                ></div>
                // Modal
                <div class="relative bg-[var(--color-bg-surface)] border border-[var(--color-border)] rounded-2xl shadow-2xl w-full max-w-md p-6">
                    <h2 class="text-lg font-semibold mb-4">"Edit Service"</h2>
                    <div class="space-y-3">
                        <div>
                            <label class="block text-xs text-[var(--color-text-muted)] mb-1">"Display Name"</label>
                            <input
                                type="text"
                                class="w-full px-3 py-2 rounded-lg bg-[var(--color-bg-primary)] border border-[var(--color-border)] text-sm"
                                prop:value=move || name.get()
                                on:input=move |ev| set_name.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="block text-xs text-[var(--color-text-muted)] mb-1">"Description"</label>
                            <input
                                type="text"
                                class="w-full px-3 py-2 rounded-lg bg-[var(--color-bg-primary)] border border-[var(--color-border)] text-sm"
                                prop:value=move || desc.get()
                                on:input=move |ev| set_desc.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="block text-xs text-[var(--color-text-muted)] mb-1">"URL"</label>
                            <input
                                type="url"
                                class="w-full px-3 py-2 rounded-lg bg-[var(--color-bg-primary)] border border-[var(--color-border)] text-sm"
                                prop:value=move || url.get()
                                on:input=move |ev| set_url.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="block text-xs text-[var(--color-text-muted)] mb-1">"Icon"</label>
                            <input
                                type="text"
                                class="w-full px-3 py-2 rounded-lg bg-[var(--color-bg-primary)] border border-[var(--color-border)] text-sm"
                                placeholder="lucide icon name or URL"
                                prop:value=move || icon.get()
                                on:input=move |ev| set_icon.set(event_target_value(&ev))
                            />
                        </div>
                    </div>
                    <div class="flex justify-end gap-2 mt-6">
                        <button
                            class="px-4 py-2 rounded-lg text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
                            on:click=move |_| on_close.run(())
                        >
                            "Cancel"
                        </button>
                        <button class="btn-primary">
                            "Save"
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
