use leptos::*;

#[component]
pub fn Sidebar() -> impl IntoView {
    let (collapsed, set_collapsed) = create_signal(false);

    let width_class = move || {
        if collapsed.get() {
            "w-16"
        } else {
            "w-56"
        }
    };

    view! {
        <aside class={move || format!(
            "h-screen {} bg-[var(--color-bg-surface)] border-r border-[var(--color-border)] \
             flex flex-col transition-all duration-200",
            width_class()
        )}>
            // Logo / Brand
            <div class="h-12 flex items-center px-4 border-b border-[var(--color-border)]">
                <span class="font-semibold text-sm tracking-tight">
                    {move || if collapsed.get() { "V" } else { "VexBoard" }}
                </span>
            </div>

            // Navigation links
            <nav class="flex-1 py-3 px-2 space-y-1">
                <a href="/" class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm hover:bg-[var(--color-bg-hover)] text-[var(--color-text-primary)]">
                    <span class="w-4 text-center">"◉"</span>
                    {move || if !collapsed.get() { Some(view! { <span>"Dashboard"</span> }) } else { None }}
                </a>
                <a href="/settings" class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm hover:bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]">
                    <span class="w-4 text-center">"⚙"</span>
                    {move || if !collapsed.get() { Some(view! { <span>"Settings"</span> }) } else { None }}
                </a>
            </nav>

            // Collapse toggle
            <div class="p-2 border-t border-[var(--color-border)]">
                <button
                    on:click=move |_| set_collapsed.update(|c| *c = !*c)
                    class="w-full px-3 py-2 rounded-lg text-xs text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)]"
                >
                    {move || if collapsed.get() { "→" } else { "← Collapse" }}
                </button>
            </div>
        </aside>
    }
}
