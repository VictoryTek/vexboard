use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Debug, Clone, serde::Deserialize)]
struct ChannelFe {
    id: i64,
    name: String,
    kind: String,
    target: String,
    /// JSON array as text (e.g. `["service.down"]`); empty array means all events.
    events: String,
    enabled: bool,
}

async fn fetch_channels() -> Vec<ChannelFe> {
    let Ok(resp) = gloo_net::http::Request::get("/api/v1/notifications/channels")
        .send()
        .await
    else {
        return Vec::new();
    };
    resp.json::<Vec<ChannelFe>>().await.unwrap_or_default()
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "discord" => "Discord",
        "ntfy" => "ntfy",
        _ => "Webhook",
    }
}

fn events_summary(raw: &str) -> String {
    let events: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    if events.is_empty() {
        "All events".to_string()
    } else {
        events
            .iter()
            .map(|e| if e == "service.down" { "Down" } else { "Up" })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[component]
pub(super) fn NotificationsSection() -> impl IntoView {
    let channels: RwSignal<Vec<ChannelFe>> = RwSignal::new(vec![]);
    let (new_name, set_new_name) = signal(String::new());
    let (new_kind, set_new_kind) = signal("webhook".to_string());
    let (new_target, set_new_target) = signal(String::new());
    let (new_secret, set_new_secret) = signal(String::new());
    let (filter_down, set_filter_down) = signal(true);
    let (filter_up, set_filter_up) = signal(false);
    let (form_error, set_form_error) = signal(String::new());
    let test_results: RwSignal<HashMap<i64, (bool, String)>> = RwSignal::new(HashMap::new());

    #[cfg(target_arch = "wasm32")]
    leptos::prelude::Effect::new(move |_| {
        spawn_local(async move {
            channels.set(fetch_channels().await);
        });
    });

    view! {
        <div>
            <p class="settings-pane-title">"Notifications"</p>
            <p class="settings-pane-sub">"Where VexBoard tells you a service went down. Add a destination, then choose which events it receives."</p>

            <div class="settings-card">
                <div class="settings-card-head">
                    {move || {
                        let n = channels.get().len();
                        format!("{n} destination{}", if n == 1 { "" } else { "s" })
                    }}
                </div>

                <For
                    each=move || channels.get()
                    key=|c| c.id
                    children=move |c| {
                        let cid = c.id;
                        let enabled = c.enabled;
                        let events_text = events_summary(&c.events);
                        let status_class = if enabled {
                            "settings-role-badge settings-role-badge-admin"
                        } else {
                            "settings-role-badge settings-role-badge-viewer"
                        };
                        let status_label = if enabled { "Enabled" } else { "Disabled" };
                        let target = c.target.clone();

                        view! {
                            <div class="settings-user-row">
                                <div class="settings-user-id">
                                    <span class="settings-user-name">{c.name.clone()}</span>
                                    <span class="settings-role-badge settings-role-badge-viewer">{kind_label(&c.kind)}</span>
                                    <span class=status_class>{status_label}</span>
                                </div>
                                <div class="settings-user-actions">
                                    <button
                                        class="btn-secondary settings-btn-sm"
                                        on:click=move |_| {
                                            spawn_local(async move {
                                                let body = serde_json::json!({"enabled": !enabled});
                                                if let Ok(req) = gloo_net::http::Request::patch(
                                                    &format!("/api/v1/notifications/channels/{cid}")
                                                ).json(&body) {
                                                    let _ = req.send().await;
                                                }
                                                channels.set(fetch_channels().await);
                                            });
                                        }
                                    >{if enabled { "Disable" } else { "Enable" }}</button>
                                    <button
                                        class="btn-secondary settings-btn-sm"
                                        on:click=move |_| {
                                            test_results.update(|m| { m.remove(&cid); });
                                            spawn_local(async move {
                                                let outcome = gloo_net::http::Request::post(
                                                    &format!("/api/v1/notifications/channels/{cid}/test")
                                                ).send().await;
                                                let result = match outcome {
                                                    Ok(resp) if resp.ok() => (false, "Test sent — check the destination.".to_string()),
                                                    Ok(resp) => {
                                                        let msg = resp.json::<serde_json::Value>().await.ok()
                                                            .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
                                                            .unwrap_or_else(|| "Delivery failed.".to_string());
                                                        (true, msg)
                                                    }
                                                    Err(_) => (true, "Could not reach the server.".to_string()),
                                                };
                                                test_results.update(|m| { m.insert(cid, result); });
                                            });
                                        }
                                    >"Test"</button>
                                    <button
                                        class="settings-btn-ghost"
                                        on:click=move |_| {
                                            spawn_local(async move {
                                                let _ = gloo_net::http::Request::delete(
                                                    &format!("/api/v1/notifications/channels/{cid}")
                                                ).send().await;
                                                test_results.update(|m| { m.remove(&cid); });
                                                channels.set(fetch_channels().await);
                                            });
                                        }
                                    >"Remove"</button>
                                </div>
                                <p class="text-xs mt-0.5" style="color:var(--color-text-muted); width:100%; margin:0.25rem 0 0;">
                                    {format!("{target} · {events_text}")}
                                </p>
                                {move || test_results.get().get(&cid).cloned().map(|(is_err, msg)| {
                                    let class = if is_err { "settings-form-error" } else { "settings-form-success" };
                                    view! { <p class=class style="width:100%; margin:0.25rem 0 0;">{msg}</p> }
                                })}
                            </div>
                        }
                    }
                />

                <div class="settings-add-user">
                    <p class="text-xs font-semibold" style="color:var(--color-text-secondary); margin:0;">"Add Destination"</p>
                    <div class="settings-add-user-fields">
                        <input
                            type="text"
                            placeholder="Name"
                            class="form-input"
                            prop:value=new_name
                            on:input=move |ev| set_new_name.set(event_target_value(&ev))
                        />
                        <select
                            class="form-input"
                            on:change=move |ev| set_new_kind.set(event_target_value(&ev))
                        >
                            <option value="webhook" selected=true>"Webhook"</option>
                            <option value="discord">"Discord"</option>
                            <option value="ntfy">"ntfy"</option>
                        </select>
                        <input
                            type="text"
                            placeholder="Target URL"
                            class="form-input"
                            style="flex:2; min-width:200px;"
                            prop:value=new_target
                            on:input=move |ev| set_new_target.set(event_target_value(&ev))
                        />
                        <Show when=move || new_kind.get() == "webhook">
                            <input
                                type="text"
                                placeholder="Signing secret (optional)"
                                class="form-input"
                                prop:value=new_secret
                                on:input=move |ev| set_new_secret.set(event_target_value(&ev))
                            />
                        </Show>
                        <button
                            class="btn-primary"
                            on:click=move |_| {
                                let name = new_name.get();
                                let target = new_target.get();
                                if name.trim().is_empty() || target.trim().is_empty() {
                                    set_form_error.set("Name and target URL are required.".to_string());
                                    return;
                                }
                                set_form_error.set(String::new());
                                let kind = new_kind.get();
                                let secret = new_secret.get();
                                let mut events = Vec::new();
                                if filter_down.get() { events.push("service.down".to_string()); }
                                if filter_up.get() { events.push("service.up".to_string()); }
                                spawn_local(async move {
                                    let body = serde_json::json!({
                                        "name": name,
                                        "kind": kind,
                                        "target": target,
                                        "secret": if secret.is_empty() { None } else { Some(secret) },
                                        "events": events,
                                    });
                                    let result = if let Ok(req) = gloo_net::http::Request::post("/api/v1/notifications/channels").json(&body) {
                                        req.send().await.ok()
                                    } else { None };
                                    if let Some(resp) = result {
                                        if resp.ok() {
                                            set_new_name.set(String::new());
                                            set_new_target.set(String::new());
                                            set_new_secret.set(String::new());
                                            channels.set(fetch_channels().await);
                                        } else if let Ok(body) = resp.json::<serde_json::Value>().await {
                                            let msg = body["error"].as_str().unwrap_or("Failed to add destination").to_string();
                                            set_form_error.set(msg);
                                        }
                                    }
                                });
                            }
                        >"Add"</button>
                    </div>
                    <div style="display:flex; gap:1rem; margin-top:0.5rem;">
                        <label style="display:flex; align-items:center; gap:0.4rem; font-size:0.78rem; color:var(--color-text-secondary);">
                            <input type="checkbox" prop:checked=filter_down on:change=move |ev| set_filter_down.set(event_target_checked(&ev)) />
                            "Notify on outage"
                        </label>
                        <label style="display:flex; align-items:center; gap:0.4rem; font-size:0.78rem; color:var(--color-text-secondary);">
                            <input type="checkbox" prop:checked=filter_up on:change=move |ev| set_filter_up.set(event_target_checked(&ev)) />
                            "Notify on recovery"
                        </label>
                    </div>
                    <p class="text-xs" style="color:var(--color-text-muted); margin:0.25rem 0 0;">
                        "Leave both unchecked to receive every event."
                    </p>
                    <Show when=move || !form_error.get().is_empty()>
                        <p class="settings-form-error">{form_error}</p>
                    </Show>
                </div>
            </div>
        </div>
    }
}
