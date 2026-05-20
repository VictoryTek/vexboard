use leptos::*;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = create_signal(String::new());
    let (password, set_password) = create_signal(String::new());
    let (error, set_error) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

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
                    // Redirect to dashboard
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
        <div class="min-h-[80vh] flex items-center justify-center">
            <div class="card w-full max-w-sm p-6">
                <h1 class="text-lg font-semibold text-center mb-6">"Sign in to VexBoard"</h1>

                {move || error.get().map(|e| view! {
                    <div class="mb-4 p-3 rounded-lg bg-[rgba(239,68,68,0.1)] text-[var(--color-danger)] text-xs">
                        {e}
                    </div>
                })}

                <form on:submit=on_submit class="space-y-4">
                    <div>
                        <label class="block text-xs text-[var(--color-text-muted)] mb-1">"Username"</label>
                        <input
                            type="text"
                            autocomplete="username"
                            required=true
                            class="w-full px-3 py-2 rounded-lg bg-[var(--color-bg-primary)] border border-[var(--color-border)] text-sm"
                            prop:value=move || username.get()
                            on:input=move |ev| set_username.set(event_target_value(&ev))
                        />
                    </div>
                    <div>
                        <label class="block text-xs text-[var(--color-text-muted)] mb-1">"Password"</label>
                        <input
                            type="password"
                            autocomplete="current-password"
                            required=true
                            class="w-full px-3 py-2 rounded-lg bg-[var(--color-bg-primary)] border border-[var(--color-border)] text-sm"
                            prop:value=move || password.get()
                            on:input=move |ev| set_password.set(event_target_value(&ev))
                        />
                    </div>
                    <button
                        type="submit"
                        class="btn-primary w-full"
                        disabled=move || loading.get()
                    >
                        {move || if loading.get() { "Signing in..." } else { "Sign In" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
