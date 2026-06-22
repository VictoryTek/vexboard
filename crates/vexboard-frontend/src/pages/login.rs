use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let username = username.get();
        let password = password.get();
        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            let result = gloo_net::http::Request::post("/api/v1/auth/login")
                .json(&serde_json::json!({
                    "username": username,
                    "password": password,
                }))
                .unwrap()
                .send()
                .await;

            set_loading.set(false);

            match result {
                Ok(resp) if resp.ok() => {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let window = web_sys::window().unwrap();
                        window.location().set_href("/").ok();
                    }
                }
                Ok(_) => {
                    set_error.set(Some("Invalid credentials".to_string()));
                }
                Err(e) => {
                    set_error.set(Some(format!("Network error: {e}")));
                }
            }
        });
    };

    view! {
        <div style="display:flex; flex-direction:column; align-items:center; justify-content:center; height:100vh; gap:1.5rem; background-color:var(--color-bg-primary)">
            // Brand
            <div style="display:flex; flex-direction:column; align-items:center; gap:0.75rem">
                <img src="/vexboard-logo.png" alt="VexBoard"
                    style="width:80px; height:80px; border-radius:18px; object-fit:contain;" />
                <div style="text-align:center;">
                    <h1 style="font-size:1.75rem; font-weight:600; letter-spacing:-0.02em; margin:0;">
                        "VexBoard"
                    </h1>
                    <p style="font-size:0.875rem; margin:0.25rem 0 0; color:var(--color-text-muted);">
                        "Sign in to your dashboard"
                    </p>
                </div>
            </div>

            // Form card
            <div class="card" style="width: 100%; max-width: 360px;">
                {move || error.get().map(|e| view! {
                    <div class="mb-4 px-3 py-2.5 rounded-lg text-xs"
                         style="background: var(--color-danger-dim); color: var(--color-danger); border: 1px solid rgba(239,68,68,0.2)">
                        {e}
                    </div>
                })}

                <form on:submit=on_submit class="space-y-4">
                    <div>
                        <label class="form-label">"Username"</label>
                        <input
                            type="text"
                            autocomplete="username"
                            required=true
                            class="form-input"
                            prop:value=move || username.get()
                            on:input=move |ev| set_username.set(event_target_value(&ev))
                        />
                    </div>
                    <div>
                        <label class="form-label">"Password"</label>
                        <input
                            type="password"
                            autocomplete="current-password"
                            required=true
                            class="form-input"
                            prop:value=move || password.get()
                            on:input=move |ev| set_password.set(event_target_value(&ev))
                        />
                    </div>
                    <button
                        type="submit"
                        class="btn-primary"
                        style="width: 100%; justify-content: center;"
                        disabled=move || loading.get()
                    >
                        {move || if loading.get() { "Signing in…" } else { "Sign In" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
