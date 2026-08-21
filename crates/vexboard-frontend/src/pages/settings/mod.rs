mod about;
mod appearance;
mod discovery;
mod security;
mod ui;
mod users;

use leptos::prelude::*;

use crate::components::sidebar::SidebarMode;
use crate::CurrentUser;

use about::AboutSection;
use appearance::AppearanceSection;
use discovery::DiscoverySection;
use security::SecuritySection;
use users::UsersSection;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    Appearance,
    Discovery,
    Security,
    Users,
    About,
}

fn tab_class(active: Section, section: Section) -> &'static str {
    if active == section {
        "settings-tab settings-tab-active"
    } else {
        "settings-tab"
    }
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let sidebar_mode =
        use_context::<ReadSignal<SidebarMode>>().expect("SidebarMode context must be provided");
    let set_sidebar_mode = use_context::<WriteSignal<SidebarMode>>()
        .expect("set_sidebar_mode context must be provided");

    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    let (active, set_active) = signal(Section::Appearance);

    view! {
        <div>
            <div class="page-header">
                <h1 class="page-title">"Settings"</h1>
            </div>

            <div class="settings-shell">
                <nav class="settings-rail">
                    <p class="settings-rail-group">"Interface"</p>
                    <button
                        class=move || tab_class(active.get(), Section::Appearance)
                        on:click=move |_| set_active.set(Section::Appearance)
                    >
                        // Sun/moon icon
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="12" r="5"/>
                            <line x1="12" y1="1" x2="12" y2="3"/>
                            <line x1="12" y1="21" x2="12" y2="23"/>
                            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
                            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
                            <line x1="1" y1="12" x2="3" y2="12"/>
                            <line x1="21" y1="12" x2="23" y2="12"/>
                            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
                            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
                        </svg>
                        "Appearance"
                    </button>

                    <p class="settings-rail-group">"Services"</p>
                    <button
                        class=move || tab_class(active.get(), Section::Discovery)
                        on:click=move |_| set_active.set(Section::Discovery)
                    >
                        // Radar/search icon
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="11" cy="11" r="8"/>
                            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                            <circle cx="11" cy="11" r="3"/>
                        </svg>
                        "Discovery"
                    </button>

                    <Show when=move || is_admin()>
                        <p class="settings-rail-group">"Administration"</p>
                        <button
                            class=move || tab_class(active.get(), Section::Security)
                            on:click=move |_| set_active.set(Section::Security)
                        >
                            // Lock icon
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
                                <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
                                <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                            </svg>
                            "Security"
                        </button>
                        <button
                            class=move || tab_class(active.get(), Section::Users)
                            on:click=move |_| set_active.set(Section::Users)
                        >
                            // Users icon
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/>
                                <circle cx="9" cy="7" r="4"/>
                                <path d="M23 21v-2a4 4 0 0 0-3-3.87"/>
                                <path d="M16 3.13a4 4 0 0 1 0 7.75"/>
                            </svg>
                            "Users"
                        </button>
                    </Show>

                    <p class="settings-rail-group">"About"</p>
                    <button
                        class=move || tab_class(active.get(), Section::About)
                        on:click=move |_| set_active.set(Section::About)
                    >
                        // Info-circle icon
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="12" r="10"/>
                            <line x1="12" y1="8" x2="12" y2="12"/>
                            <line x1="12" y1="16" x2="12.01" y2="16"/>
                        </svg>
                        "About"
                    </button>
                </nav>

                <div class="settings-pane">
                    <Show when=move || active.get() == Section::Appearance>
                        <AppearanceSection sidebar_mode=sidebar_mode set_sidebar_mode=set_sidebar_mode />
                    </Show>
                    <Show when=move || active.get() == Section::Discovery>
                        <DiscoverySection />
                    </Show>
                    <Show when=move || active.get() == Section::Security && is_admin()>
                        <SecuritySection />
                    </Show>
                    <Show when=move || active.get() == Section::Users && is_admin()>
                        <UsersSection />
                    </Show>
                    <Show when=move || active.get() == Section::About>
                        <AboutSection />
                    </Show>
                </div>
            </div>
        </div>
    }
}
