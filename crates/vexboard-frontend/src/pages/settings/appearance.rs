use leptos::prelude::*;

use crate::components::sidebar::{save_sidebar_mode_to_storage, SidebarMode};

use super::ui::{card, row_stack};

#[component]
pub(super) fn AppearanceSection(
    sidebar_mode: ReadSignal<SidebarMode>,
    set_sidebar_mode: WriteSignal<SidebarMode>,
) -> impl IntoView {
    let toggle_theme = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let win = web_sys::window().unwrap();
            let doc = win.document().unwrap();
            let html = doc.document_element().unwrap();
            let store = win.local_storage().ok().flatten();
            let is_dark =
                html.class_list().contains("dark") || !html.class_list().contains("light");
            if is_dark {
                html.class_list().remove_1("dark").ok();
                html.class_list().add_1("light").ok();
                if let Some(s) = &store {
                    let _ = s.set_item("vexboard-theme", "light");
                }
            } else {
                html.class_list().remove_1("light").ok();
                html.class_list().add_1("dark").ok();
                if let Some(s) = &store {
                    let _ = s.set_item("vexboard-theme", "dark");
                }
            }
        }
    };

    view! {
        <div>
            <p class="settings-pane-title">"Appearance"</p>
            <p class="settings-pane-sub">"How VexBoard looks in this browser."</p>

            {card("Theme", row_stack(
                "Color scheme",
                "Switch between dark and light mode.",
                view! {
                    <button class="btn-secondary" on:click=toggle_theme>"Toggle Theme"</button>
                },
            ))}

            {card("Layout", row_stack(
                "Sidebar",
                "Choose how the sidebar behaves.",
                view! {
                    <div class="settings-option-row">
                        {[
                            (SidebarMode::HoverExpand, "Hover Expand", "Collapsed by default, expands on hover."),
                            (SidebarMode::AlwaysExpanded, "Always Expanded", "Sidebar always shows labels."),
                            (SidebarMode::AlwaysCollapsed, "Always Collapsed", "Sidebar shows icons only."),
                        ].into_iter().map(|(mode, label, desc)| {
                            let mode_for_class = mode.clone();
                            let mode_for_click = mode.clone();
                            view! {
                                <button
                                    class=move || {
                                        if sidebar_mode.get() == mode_for_class {
                                            "settings-nav-option-active"
                                        } else {
                                            "settings-nav-option"
                                        }
                                    }
                                    on:click=move |_| {
                                        let m = mode_for_click.clone();
                                        save_sidebar_mode_to_storage(&m);
                                        set_sidebar_mode.set(m);
                                    }
                                >
                                    <span class="settings-nav-dot"></span>
                                    <div>
                                        <p class="text-sm font-medium">{label}</p>
                                        <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">{desc}</p>
                                    </div>
                                </button>
                            }
                        }).collect_view()}
                    </div>
                },
            ))}
        </div>
    }
}
