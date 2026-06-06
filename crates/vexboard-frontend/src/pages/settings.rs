use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::sidebar::{save_sidebar_mode_to_storage, SidebarMode};
use crate::CurrentUser;

#[derive(Debug, Clone, serde::Deserialize)]
struct UserRecord {
    id: i64,
    username: String,
    role: String,
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
    let current_username = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.username)
            .unwrap_or_default()
    };

    let users: RwSignal<Vec<UserRecord>> = RwSignal::new(vec![]);
    let (new_username, set_new_username) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (new_role, set_new_role) = signal("viewer".to_string());
    let (user_error, set_user_error) = signal(String::new());

    // Load users when the component mounts (admin only).
    #[cfg(target_arch = "wasm32")]
    leptos::prelude::Effect::new(move |_| {
        if is_admin() {
            spawn_local(async move {
                if let Ok(resp) = gloo_net::http::Request::get("/api/v1/users").send().await {
                    if let Ok(list) = resp.json::<Vec<UserRecord>>().await {
                        users.set(list);
                    }
                }
            });
        }
    });

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
                                    let win = web_sys::window().unwrap();
                                    let doc = win.document().unwrap();
                                    let html = doc.document_element().unwrap();
                                    let store = win.local_storage().ok().flatten();
                                    let is_dark = html.class_list().contains("dark")
                                        || !html.class_list().contains("light");
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

                // User Management (admin only)
                <Show when=move || is_admin()>
                    <div class="card">
                        <h2 class="text-sm font-semibold mb-3"
                            style="color: var(--color-text-primary)">"User Management"</h2>

                        // User list
                        <div style="margin-bottom:1rem;">
                            <For
                                each=move || users.get()
                                key=|u| u.id
                                children=move |u| {
                                    let uid = u.id;
                                    let uname = u.username.clone();
                                    let urole = u.role.clone();
                                    let is_self = uname == current_username();
                                    let role_color = if urole == "admin" {
                                        "var(--color-accent)"
                                    } else {
                                        "var(--color-text-muted)"
                                    };
                                    view! {
                                        <div style="display:flex; align-items:center; justify-content:space-between; \
                                                    padding:0.4rem 0; border-bottom:1px solid var(--color-border);">
                                            <div style="display:flex; align-items:center; gap:0.5rem;">
                                                <span style="font-size:0.82rem; color:var(--color-text-primary);">{uname.clone()}</span>
                                                <span style=format!(
                                                    "font-size:0.65rem; font-weight:700; text-transform:uppercase; \
                                                     letter-spacing:0.04em; color:{role_color}; \
                                                     background:{role_color}22; border:1px solid {role_color}44; \
                                                     border-radius:20px; padding:1px 7px;"
                                                )>{urole.clone()}</span>
                                            </div>
                                            <div style="display:flex; gap:0.5rem; align-items:center;">
                                                // Toggle role button (not for self)
                                                {(!is_self).then(|| {
                                                    let new_r = if urole == "admin" { "viewer" } else { "admin" };
                                                    let label = if urole == "admin" { "→ Viewer" } else { "→ Admin" };
                                                    view! {
                                                        <button
                                                            class="btn-secondary"
                                                            style="font-size:0.7rem; padding:0.2rem 0.5rem;"
                                                            on:click=move |_| {
                                                                spawn_local(async move {
                                                                    let body = serde_json::json!({"role": new_r});
                                                                    if let Ok(req) = gloo_net::http::Request::patch(
                                                                        &format!("/api/v1/users/{uid}")
                                                                    ).json(&body) {
                                                                        let _ = req.send().await;
                                                                    }
                                                                    if let Ok(resp) = gloo_net::http::Request::get("/api/v1/users").send().await {
                                                                        if let Ok(list) = resp.json::<Vec<UserRecord>>().await {
                                                                            users.set(list);
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        >{label}</button>
                                                    }
                                                })}
                                                // Delete button (not for self)
                                                {(!is_self).then(|| view! {
                                                    <button
                                                        style="background:none; border:none; cursor:pointer; \
                                                               color:var(--color-text-muted); font-size:0.7rem; \
                                                               padding:0.2rem 0.4rem;"
                                                        onmouseover="this.style.color='var(--color-danger)'"
                                                        onmouseout="this.style.color='var(--color-text-muted)'"
                                                        on:click=move |_| {
                                                            spawn_local(async move {
                                                                let _ = gloo_net::http::Request::delete(
                                                                    &format!("/api/v1/users/{uid}")
                                                                ).send().await;
                                                                if let Ok(resp) = gloo_net::http::Request::get("/api/v1/users").send().await {
                                                                    if let Ok(list) = resp.json::<Vec<UserRecord>>().await {
                                                                        users.set(list);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    >"Remove"</button>
                                                })}
                                            </div>
                                        </div>
                                    }
                                }
                            />
                        </div>

                        // Create user form
                        <div style="display:flex; flex-direction:column; gap:0.5rem; margin-top:0.75rem;">
                            <p class="text-xs font-semibold" style="color:var(--color-text-secondary);">"Add User"</p>
                            <div style="display:flex; gap:0.5rem; flex-wrap:wrap;">
                                <input
                                    type="text"
                                    placeholder="Username"
                                    class="input"
                                    style="flex:1; min-width:120px;"
                                    prop:value=new_username
                                    on:input=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                            set_new_username.set(el.value());
                                        }
                                    }
                                />
                                <input
                                    type="password"
                                    placeholder="Password (min 8)"
                                    class="input"
                                    style="flex:1; min-width:120px;"
                                    prop:value=new_password
                                    on:input=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                            set_new_password.set(el.value());
                                        }
                                    }
                                />
                                <select
                                    class="input"
                                    style="flex:0 0 auto;"
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                            set_new_role.set(el.value());
                                        }
                                    }
                                >
                                    <option value="viewer" selected=true>"Viewer"</option>
                                    <option value="admin">"Admin"</option>
                                </select>
                                <button
                                    class="btn-primary"
                                    on:click=move |_| {
                                        let uname = new_username.get();
                                        let pwd = new_password.get();
                                        let role = new_role.get();
                                        if uname.trim().is_empty() || pwd.is_empty() {
                                            set_user_error.set("Username and password are required.".to_string());
                                            return;
                                        }
                                        set_user_error.set(String::new());
                                        spawn_local(async move {
                                            let body = serde_json::json!({"username": uname, "password": pwd, "role": role});
                                            let result = if let Ok(req) = gloo_net::http::Request::post("/api/v1/users").json(&body) {
                                                req.send().await.ok()
                                            } else { None };
                                            if let Some(resp) = result {
                                                if resp.ok() {
                                                    set_new_username.set(String::new());
                                                    set_new_password.set(String::new());
                                                    if let Ok(resp2) = gloo_net::http::Request::get("/api/v1/users").send().await {
                                                        if let Ok(list) = resp2.json::<Vec<UserRecord>>().await {
                                                            users.set(list);
                                                        }
                                                    }
                                                } else if let Ok(body) = resp.json::<serde_json::Value>().await {
                                                    let msg = body["error"].as_str().unwrap_or("Failed to create user").to_string();
                                                    set_user_error.set(msg);
                                                }
                                            }
                                        });
                                    }
                                >"Add"</button>
                            </div>
                            <Show when=move || !user_error.get().is_empty()>
                                <p style="font-size:0.75rem; color:var(--color-danger);">{user_error}</p>
                            </Show>
                        </div>
                    </div>
                </Show>

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
