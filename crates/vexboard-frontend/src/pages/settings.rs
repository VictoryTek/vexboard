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

#[derive(Debug, Clone, serde::Deserialize)]
struct AuthModeStatus {
    stored_mode: String,
    restart_required: bool,
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

    let stored_mode: RwSignal<String> = RwSignal::new("session".to_string());
    let restart_required: RwSignal<bool> = RwSignal::new(false);

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
            spawn_local(async move {
                if let Ok(resp) = gloo_net::http::Request::get("/api/v1/settings/auth-mode")
                    .send()
                    .await
                {
                    if let Ok(status) = resp.json::<AuthModeStatus>().await {
                        stored_mode.set(status.stored_mode);
                        restart_required.set(status.restart_required);
                    }
                }
            });
        }
    });

    let set_login_required = move |required: bool| {
        let mode = if required { "session" } else { "none" }.to_string();
        spawn_local(async move {
            let body = serde_json::json!({"mode": mode});
            if let Ok(req) =
                gloo_net::http::Request::patch("/api/v1/settings/auth-mode").json(&body)
            {
                if let Ok(resp) = req.send().await {
                    if let Ok(status) = resp.json::<AuthModeStatus>().await {
                        stored_mode.set(status.stored_mode);
                        restart_required.set(status.restart_required);
                    }
                }
            }
        });
    };

    view! {
        <div>
            <div class="page-header">
                <h1 class="page-title">"Settings"</h1>
            </div>

            <div class="settings-about-banner">
                // Info-circle icon
                <svg class="settings-section-icon" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="2" stroke-linecap="round"
                     stroke-linejoin="round">
                    <circle cx="12" cy="12" r="10"/>
                    <line x1="12" y1="8" x2="12" y2="12"/>
                    <line x1="12" y1="16" x2="12.01" y2="16"/>
                </svg>
                <p class="settings-about-text">
                    <strong>{concat!("VexBoard v", env!("CARGO_PKG_VERSION"))}</strong>
                    " — Self-hosted server dashboard for NixOS and systemd."
                </p>
            </div>

            <div class="settings-list">
                // ── Appearance ──────────────────────────────────────────
                <div class="settings-row">
                    <div class="settings-row-label">
                        <div class="settings-section-header">
                            // Sun/moon icon
                            <svg class="settings-section-icon" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                 stroke-linejoin="round">
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
                        </div>
                        <p class="text-xs" style="color: var(--color-text-muted)">
                            "Toggle between dark and light mode."
                        </p>
                    </div>
                    <div class="settings-row-control" style="display: flex; justify-content: flex-end;">
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

                // ── Navigation Sidebar ───────────────────────────────────
                <div class="settings-row">
                    <div class="settings-row-label">
                        <div class="settings-section-header">
                            // Layout/sidebar icon
                            <svg class="settings-section-icon" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                 stroke-linejoin="round">
                                <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
                                <line x1="9" y1="3" x2="9" y2="21"/>
                            </svg>
                            "Navigation Sidebar"
                        </div>
                        <p class="text-xs" style="color: var(--color-text-muted)">
                            "Choose how the sidebar behaves."
                        </p>
                    </div>
                    <div class="settings-row-control settings-option-row">
                        {[
                            (SidebarMode::HoverExpand,     "Hover Expand",     "Collapsed by default, expands on hover."),
                            (SidebarMode::AlwaysExpanded,  "Always Expanded",  "Sidebar always shows labels."),
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
                </div>

                // ── Service Discovery ────────────────────────────────────
                <div class="settings-row">
                    <div class="settings-row-label">
                        <div class="settings-section-header">
                            // Radar/search icon
                            <svg class="settings-section-icon" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                 stroke-linejoin="round">
                                <circle cx="11" cy="11" r="8"/>
                                <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                                <circle cx="11" cy="11" r="3"/>
                            </svg>
                            "Service Discovery"
                        </div>
                        <p class="text-xs" style="color: var(--color-text-muted)">
                            "How newly found services are surfaced."
                        </p>
                    </div>
                    <div class="settings-row-control">
                        <p class="text-xs leading-relaxed" style="color: var(--color-text-secondary)">
                            "VexBoard automatically discovers running systemd services via D-Bus. \
                             Discovered services appear in the dashboard for you to claim or dismiss."
                        </p>
                    </div>
                </div>

                // ── Authentication (admin only) ───────────────────────────
                <Show when=move || is_admin()>
                    <div class="settings-row">
                        <div class="settings-row-label">
                            <div class="settings-section-header">
                                // Lock icon
                                <svg class="settings-section-icon" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                     stroke-linejoin="round">
                                    <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
                                    <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                                </svg>
                                "Login"
                            </div>
                            <p class="text-xs" style="color: var(--color-text-muted)">
                                "Whether VexBoard asks for a username and password."
                            </p>
                        </div>
                        <div class="settings-row-control settings-option-row">
                            <button
                                class=move || {
                                    if stored_mode.get() == "session" {
                                        "settings-nav-option-active"
                                    } else {
                                        "settings-nav-option"
                                    }
                                }
                                on:click=move |_| set_login_required(true)
                            >
                                <span class="settings-nav-dot"></span>
                                <div>
                                    <p class="text-sm font-medium">"Require Login"</p>
                                    <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">
                                        "Recommended. Visitors must sign in to view or manage this dashboard."
                                    </p>
                                </div>
                            </button>
                            <button
                                class=move || {
                                    if stored_mode.get() == "none" {
                                        "settings-nav-option-active"
                                    } else {
                                        "settings-nav-option"
                                    }
                                }
                                on:click=move |_| set_login_required(false)
                            >
                                <span class="settings-nav-dot"></span>
                                <div>
                                    <p class="text-sm font-medium">"Disable Login"</p>
                                    <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">
                                        "Anyone who can reach this server gets full access, no password needed. \
                                         Only use this if your network already restricts access, e.g. Tailscale-only or an isolated LAN."
                                    </p>
                                </div>
                            </button>
                            <Show when=move || restart_required.get()>
                                <p class="text-xs" style="color: var(--color-accent); margin-top: 0.5rem;">
                                    "Saved — restart VexBoard for this change to take effect."
                                </p>
                            </Show>
                        </div>
                    </div>
                </Show>

                // ── User Management (admin only) ─────────────────────────
                <Show when=move || is_admin()>
                    <div class="settings-row">
                        <div class="settings-row-label">
                        <div class="settings-section-header">
                            // Users icon
                            <svg class="settings-section-icon" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                 stroke-linejoin="round">
                                <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/>
                                <circle cx="9" cy="7" r="4"/>
                                <path d="M23 21v-2a4 4 0 0 0-3-3.87"/>
                                <path d="M16 3.13a4 4 0 0 1 0 7.75"/>
                            </svg>
                            "User Management"
                        </div>
                        <p class="text-xs" style="color: var(--color-text-muted)">
                            "Admins only. Add, promote, or remove accounts."
                        </p>
                        </div>
                        <div class="settings-row-control">

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
                                                    padding:0.5rem 0; border-bottom:1px solid var(--color-border);">
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
                                    class="form-input"
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
                                    class="form-input"
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
                                    class="form-input"
                                    style="flex:0 0 auto;"
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
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
                    </div>
                </Show>
            </div>
        </div>
    }
}
