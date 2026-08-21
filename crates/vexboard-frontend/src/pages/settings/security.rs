use leptos::prelude::*;
use leptos::task::spawn_local;

use super::ui::{card, row_stack};

#[derive(Debug, Clone, serde::Deserialize)]
struct AuthModeStatus {
    stored_mode: String,
    restart_required: bool,
}

#[component]
pub(super) fn SecuritySection() -> impl IntoView {
    let stored_mode: RwSignal<String> = RwSignal::new("session".to_string());
    let restart_required: RwSignal<bool> = RwSignal::new(false);

    #[cfg(target_arch = "wasm32")]
    leptos::prelude::Effect::new(move |_| {
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
            <p class="settings-pane-title">"Security"</p>
            <p class="settings-pane-sub">"Whether VexBoard asks for a username and password."</p>

            {card("Access", row_stack(
                "Login",
                "Recommended: require sign-in for anyone reaching this dashboard.",
                view! {
                    <div class="settings-option-row">
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
                                    "Visitors must sign in to view or manage this dashboard."
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
                },
            ))}
        </div>
    }
}
