use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SidebarMode {
    #[default]
    HoverExpand,
    AlwaysExpanded,
    AlwaysCollapsed,
}

#[cfg(target_arch = "wasm32")]
pub fn load_sidebar_mode_from_storage() -> SidebarMode {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("vexboard_sidebar_mode").ok().flatten())
        .map(|v| match v.as_str() {
            "always_expanded" => SidebarMode::AlwaysExpanded,
            "always_collapsed" => SidebarMode::AlwaysCollapsed,
            _ => SidebarMode::HoverExpand,
        })
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn load_sidebar_mode_from_storage() -> SidebarMode {
    SidebarMode::HoverExpand
}

#[cfg(target_arch = "wasm32")]
pub fn save_sidebar_mode_to_storage(mode: &SidebarMode) {
    let val = match mode {
        SidebarMode::AlwaysExpanded => "always_expanded",
        SidebarMode::AlwaysCollapsed => "always_collapsed",
        SidebarMode::HoverExpand => "hover_expand",
    };
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .map(|s| s.set_item("vexboard_sidebar_mode", val).ok());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_sidebar_mode_to_storage(_mode: &SidebarMode) {}

#[component]
pub fn Sidebar() -> impl IntoView {
    let sidebar_mode =
        use_context::<ReadSignal<SidebarMode>>().expect("SidebarMode context must be provided");
    let (hovered, set_hovered) = signal(false);
    let location = use_location();
    let pathname = location.pathname;

    let is_expanded = move || match sidebar_mode.get() {
        SidebarMode::AlwaysExpanded => true,
        SidebarMode::AlwaysCollapsed => false,
        SidebarMode::HoverExpand => hovered.get(),
    };

    view! {
        <aside
            class="sidebar"
            style=move || format!("width: {}px", if is_expanded() { 220 } else { 60 })
            on:mouseenter=move |_| set_hovered.set(true)
            on:mouseleave=move |_| set_hovered.set(false)
        >
            // Logo / brand
            <div class="sidebar-logo">
                <div class="sidebar-logo-icon">
                    <img src="/vexboard-logo.png" alt="VexBoard"
                         style="width:28px;height:28px;object-fit:contain;" />
                </div>
                {move || is_expanded().then(|| view! {
                    <span class="sidebar-logo-text">"VexBoard"</span>
                })}
            </div>

            // Navigation
            <nav style="flex:1; overflow-y:auto; padding:0.75rem 0.5rem;" class="space-y-0.5">
                <a href="/"
                   class=move || if pathname.get() == "/" { "nav-item-active" } else { "nav-item" }>
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1"/>
                        <rect x="14" y="3" width="7" height="7" rx="1"/>
                        <rect x="3" y="14" width="7" height="7" rx="1"/>
                        <rect x="14" y="14" width="7" height="7" rx="1"/>
                    </svg>
                    {move || is_expanded().then(|| view! { <span>"Dashboard"</span> })}
                </a>

                <a href="/discovered"
                   class=move || {
                       if pathname.get().starts_with("/discovered") { "nav-item-active" } else { "nav-item" }
                   }>
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M3 7h18"/>
                        <path d="M6 12h12"/>
                        <path d="M9 17h6"/>
                    </svg>
                    {move || is_expanded().then(|| view! { <span>"Discovered"</span> })}
                </a>
            </nav>

            // Settings cog — pinned to bottom
            <div class="sidebar-footer">
                <a href="/settings"
                   class=move || {
                       if pathname.get().starts_with("/settings") { "nav-item-active" } else { "nav-item" }
                   }
                   style="width: 100%;">
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="3"/>
                        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                    </svg>
                    {move || is_expanded().then(|| view! { <span>"Settings"</span> })}
                </a>
            </div>
        </aside>
    }
}
