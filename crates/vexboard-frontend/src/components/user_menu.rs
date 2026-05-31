use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
struct MeResponse {
    username: String,
    auth_mode: String,
}

#[derive(Deserialize)]
struct MeWrapper {
    user: MeResponse,
}

async fn fetch_me() -> MeResponse {
    match gloo_net::http::Request::get("/api/v1/auth/me").send().await {
        Ok(r) if r.ok() => r
            .json::<MeWrapper>()
            .await
            .map(|w| w.user)
            .unwrap_or_default(),
        _ => MeResponse::default(),
    }
}

#[component]
pub fn UserMenu() -> impl IntoView {
    let me = LocalResource::new(|| async move { fetch_me().await });

    let (dropdown_open, set_dropdown_open) = signal(false);
    let (modal_open, set_modal_open) = signal(false);

    let (current_password, set_current_password) = signal(String::new());
    let (new_username, set_new_username) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (save_error, set_save_error) = signal(Option::<String>::None);
    let (save_success, set_save_success) = signal(false);

    let on_logout = move |_: web_sys::MouseEvent| {
        spawn_local(async move {
            let _ = gloo_net::http::Request::post("/api/v1/auth/logout")
                .send()
                .await;
            #[cfg(target_arch = "wasm32")]
            web_sys::window()
                .unwrap()
                .location()
                .set_href("/login")
                .ok();
        });
    };

    let on_save = move |_: web_sys::MouseEvent| {
        let auth_mode = me.get().map(|m| m.auth_mode.clone()).unwrap_or_default();
        let current_pw = current_password.get();
        let nu = new_username.get();
        let np = new_password.get();
        let cp = confirm_password.get();

        if auth_mode == "local" && !np.is_empty() && np != cp {
            set_save_error.set(Some("Passwords do not match.".to_string()));
            return;
        }

        set_save_error.set(None);

        spawn_local(async move {
            let mut body = serde_json::json!({ "current_password": current_pw });
            if auth_mode == "local" {
                if !nu.is_empty() {
                    body["new_username"] = serde_json::Value::String(nu);
                }
                if !np.is_empty() {
                    body["new_password"] = serde_json::Value::String(np);
                }
            }

            match gloo_net::http::Request::patch("/api/v1/auth/me")
                .json(&body)
                .unwrap()
                .send()
                .await
            {
                Ok(r) if r.ok() => {
                    set_save_success.set(true);
                    gloo_timers::callback::Timeout::new(1500, move || {
                        #[cfg(target_arch = "wasm32")]
                        web_sys::window()
                            .unwrap()
                            .location()
                            .set_href("/login")
                            .ok();
                    })
                    .forget();
                }
                Ok(r) => {
                    let msg = r
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    set_save_error.set(Some(msg));
                }
                Err(e) => {
                    set_save_error.set(Some(format!("Network error: {e}")));
                }
            }
        });
    };

    view! {
        <div class="user-menu">
            <button class="user-menu-trigger"
                on:click=move |_| set_dropdown_open.update(|v| *v = !*v)>
                <span class="user-menu-avatar">
                    {move || {
                        me.get()
                            .map(|m| {
                                m.username
                                    .chars()
                                    .next()
                                    .map(|c| c.to_uppercase().to_string())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default()
                    }}
                </span>
                {move || {
                    me.get().map(|m| view! { <span>{m.username.clone()}</span> })
                }}
            </button>

            <div class=move || {
                if dropdown_open.get() { "user-menu-dropdown open" } else { "user-menu-dropdown" }
            }>
                <div class="user-menu-dropdown-username">
                    {move || me.get().map(|m| m.username.clone()).unwrap_or_default()}
                </div>
                <button class="user-menu-item"
                    on:click=move |_| {
                        set_dropdown_open.set(false);
                        set_modal_open.set(true);
                    }>
                    "Account Settings"
                </button>
                <button class="user-menu-item danger" on:click=on_logout>
                    "Logout"
                </button>
            </div>
        </div>

        <Show when=move || modal_open.get()>
            <div class="acct-modal-overlay">
                <div class="acct-modal">
                    <h3>"Account Settings"</h3>
                    <div class="form-group">
                        <label>"Current Password"</label>
                        <input type="password"
                            prop:value=move || current_password.get()
                            on:input=move |ev| set_current_password.set(event_target_value(&ev))
                        />
                    </div>
                    {move || {
                        me.get().map(|m| {
                            if m.auth_mode == "local" {
                                Either::Left(view! {
                                    <div>
                                        <div class="form-group">
                                            <label>"New Username"</label>
                                            <input type="text"
                                                prop:value=move || new_username.get()
                                                on:input=move |ev| set_new_username.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div class="form-group">
                                            <label>"New Password"</label>
                                            <input type="password"
                                                prop:value=move || new_password.get()
                                                on:input=move |ev| set_new_password.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div class="form-group">
                                            <label>"Confirm Password"</label>
                                            <input type="password"
                                                prop:value=move || confirm_password.get()
                                                on:input=move |ev| set_confirm_password.set(event_target_value(&ev))
                                            />
                                        </div>
                                    </div>
                                })
                            } else {
                                Either::Right(view! {
                                    <p class="pam-notice">
                                        "Username and password are managed by your operating system."
                                    </p>
                                })
                            }
                        })
                    }}
                    {move || save_error.get().map(|e| view! { <p class="error-msg">{e}</p> })}
                    {move || save_success.get().then(|| view! {
                        <p class="success-msg">"Saved. Redirecting to login..."</p>
                    })}
                    <div class="modal-actions">
                        <button class="btn-secondary"
                            on:click=move |_| {
                                set_modal_open.set(false);
                                set_save_error.set(None);
                                set_save_success.set(false);
                            }>
                            "Cancel"
                        </button>
                        {move || {
                            me.get()
                                .filter(|m| m.auth_mode == "local")
                                .map(|_| {
                                    view! {
                                        <button class="btn-primary" on:click=on_save>
                                            "Save"
                                        </button>
                                    }
                                })
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}
