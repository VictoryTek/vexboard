use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::CurrentUser;

#[derive(Debug, Clone, serde::Deserialize)]
struct UserRecord {
    id: i64,
    username: String,
    role: String,
}

#[component]
pub(super) fn UsersSection() -> impl IntoView {
    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
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

    #[cfg(target_arch = "wasm32")]
    leptos::prelude::Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(resp) = gloo_net::http::Request::get("/api/v1/users").send().await {
                if let Ok(list) = resp.json::<Vec<UserRecord>>().await {
                    users.set(list);
                }
            }
        });
    });

    view! {
        <div>
            <p class="settings-pane-title">"Users"</p>
            <p class="settings-pane-sub">"Admins can change anything. Viewers see the dashboard and nothing else."</p>

            <div class="settings-card">
                <div class="settings-card-head">
                    {move || {
                        let n = users.get().len();
                        format!("{n} account{}", if n == 1 { "" } else { "s" })
                    }}
                </div>

                <For
                    each=move || users.get()
                    key=|u| u.id
                    children=move |u| {
                        let uid = u.id;
                        let uname = u.username.clone();
                        let urole = u.role.clone();
                        let is_self = uname == current_username();
                        let badge_class = if urole == "admin" {
                            "settings-role-badge settings-role-badge-admin"
                        } else {
                            "settings-role-badge settings-role-badge-viewer"
                        };
                        view! {
                            <div class="settings-user-row">
                                <div class="settings-user-id">
                                    <span class="settings-user-name">{uname.clone()}</span>
                                    <span class=badge_class>{urole.clone()}</span>
                                </div>
                                <div class="settings-user-actions">
                                    {(!is_self).then(|| {
                                        let new_r = if urole == "admin" { "viewer" } else { "admin" };
                                        let label = if urole == "admin" { "→ Viewer" } else { "→ Admin" };
                                        view! {
                                            <button
                                                class="btn-secondary settings-btn-sm"
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
                                            class="settings-btn-ghost"
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

                <div class="settings-add-user">
                    <p class="text-xs font-semibold" style="color:var(--color-text-secondary); margin:0;">"Add User"</p>
                    <div class="settings-add-user-fields">
                        <input
                            type="text"
                            placeholder="Username"
                            class="form-input"
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
                        <p class="settings-form-error">{user_error}</p>
                    </Show>
                </div>
            </div>
        </div>
    }
}
