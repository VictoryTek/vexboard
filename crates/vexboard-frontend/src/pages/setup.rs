use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn SetupPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (confirm, set_confirm) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let u = username.get();
        let p = password.get();
        let c = confirm.get();
        if p != c {
            set_error.set(Some("Passwords do not match".into()));
            return;
        }
        if p.len() < 8 {
            set_error.set(Some("Password must be at least 8 characters".into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            let result = gloo_net::http::Request::post("/api/v1/setup")
                .json(&serde_json::json!({ "username": u, "password": p }))
                .unwrap()
                .send()
                .await;
            set_loading.set(false);
            match result {
                Ok(resp) if resp.ok() => {
                    #[cfg(target_arch = "wasm32")]
                    web_sys::window()
                        .unwrap()
                        .location()
                        .set_href("/login")
                        .ok();
                }
                Ok(resp) if resp.status() == 409 => {
                    set_error.set(Some("Setup already completed — please log in.".into()));
                    #[cfg(target_arch = "wasm32")]
                    web_sys::window()
                        .unwrap()
                        .location()
                        .set_href("/login")
                        .ok();
                }
                Ok(_) => set_error.set(Some("Setup failed — please try again.".into())),
                Err(e) => set_error.set(Some(format!("Network error: {e}"))),
            }
        });
    };

    view! {
        <div style="display:flex; flex-direction:column; align-items:center; justify-content:center; height:100vh; gap:1.5rem; background-color:var(--color-bg-primary)">
            <div class="text-center">
                <h1 class="text-xl font-semibold tracking-tight">"Welcome to VexBoard"</h1>
                <p class="text-xs mt-1" style="color: var(--color-text-muted)">
                    "Create your admin account to get started."
                </p>
            </div>
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
                        <input type="text" class="form-input" required=true
                               prop:value=move || username.get()
                               on:input=move |ev| set_username.set(event_target_value(&ev)) />
                    </div>
                    <div>
                        <label class="form-label">"Password"</label>
                        <input type="password" class="form-input" required=true
                               prop:value=move || password.get()
                               on:input=move |ev| set_password.set(event_target_value(&ev)) />
                    </div>
                    <div>
                        <label class="form-label">"Confirm Password"</label>
                        <input type="password" class="form-input" required=true
                               prop:value=move || confirm.get()
                               on:input=move |ev| set_confirm.set(event_target_value(&ev)) />
                    </div>
                    <button type="submit" class="btn-primary"
                            style="width: 100%; justify-content: center;"
                            disabled=move || loading.get()>
                        {move || if loading.get() { "Creating account…" } else { "Create Admin Account" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
