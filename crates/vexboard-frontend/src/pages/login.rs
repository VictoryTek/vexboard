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
                <div style="
                    width: 52px; height: 52px; border-radius: 14px;
                    background: linear-gradient(135deg, #3b82f6 0%, #6366f1 100%);
                    display: flex; align-items: center; justify-content: center;
                    box-shadow: 0 0 28px rgba(99,102,241,0.5);
                ">
                    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="white"
                         stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1.5"/>
                        <rect x="14" y="3" width="7" height="7" rx="1.5"/>
                        <rect x="3" y="14" width="7" height="7" rx="1.5"/>
                        <rect x="14" y="14" width="7" height="7" rx="1.5"/>
                    </svg>
                </div>
                <div class="text-center">
                    <h1 class="text-xl font-semibold tracking-tight"
                        style="letter-spacing: -0.02em">"VexBoard"</h1>
                    <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">
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
