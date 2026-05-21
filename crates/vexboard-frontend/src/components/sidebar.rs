use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[component]
pub fn Sidebar() -> impl IntoView {
    let (collapsed, set_collapsed) = signal(false);
    let location = use_location();
    let pathname = location.pathname;

    view! {
        <aside
            class="sidebar"
            style=move || format!("width: {}px", if collapsed.get() { 60 } else { 220 })
        >
            // Logo / brand
            <div class="sidebar-logo">
                <div class="sidebar-logo-icon">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1.5"/>
                        <rect x="14" y="3" width="7" height="7" rx="1.5"/>
                        <rect x="3" y="14" width="7" height="7" rx="1.5"/>
                        <rect x="14" y="14" width="7" height="7" rx="1.5"/>
                    </svg>
                </div>
                {move || (!collapsed.get()).then(|| view! {
                    <span class="sidebar-logo-text">"VexBoard"</span>
                })}
            </div>

            // Navigation
            <nav class="flex-1 py-3 px-2 space-y-0.5 overflow-y-auto">
                <a href="/" class=move || if pathname.get() == "/" { "nav-item-active" } else { "nav-item" }>
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1"/>
                        <rect x="14" y="3" width="7" height="7" rx="1"/>
                        <rect x="3" y="14" width="7" height="7" rx="1"/>
                        <rect x="14" y="14" width="7" height="7" rx="1"/>
                    </svg>
                    {move || (!collapsed.get()).then(|| view! { <span>"Dashboard"</span> })}
                </a>

                <a href="/settings" class=move || if pathname.get().starts_with("/settings") { "nav-item-active" } else { "nav-item" }>
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="3"/>
                        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                    </svg>
                    {move || (!collapsed.get()).then(|| view! { <span>"Settings"</span> })}
                </a>
            </nav>

            // Collapse toggle
            <div class="sidebar-footer">
                <button
                    class="nav-item"
                    style="width: 100%;"
                    on:click=move |_| set_collapsed.update(|c| *c = !*c)
                >
                    <svg
                        class="nav-icon"
                        style=move || format!(
                            "transition: transform 200ms;{}",
                            if collapsed.get() { " transform: rotate(180deg);" } else { "" }
                        )
                        viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                        stroke-linecap="round" stroke-linejoin="round"
                    >
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                    {move || (!collapsed.get()).then(|| view! { <span>"Collapse"</span> })}
                </button>
            </div>
        </aside>
    }
}
