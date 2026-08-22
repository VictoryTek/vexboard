use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::CurrentUser;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ControlAction {
    Start,
    Stop,
    Restart,
}

impl ControlAction {
    fn as_str(self) -> &'static str {
        match self {
            ControlAction::Start => "start",
            ControlAction::Stop => "stop",
            ControlAction::Restart => "restart",
        }
    }

    fn label(self) -> &'static str {
        match self {
            ControlAction::Start => "Start",
            ControlAction::Stop => "Stop",
            ControlAction::Restart => "Restart",
        }
    }

    fn confirm_label(self) -> &'static str {
        match self {
            ControlAction::Start => "Start",
            ControlAction::Stop => "Confirm Stop?",
            ControlAction::Restart => "Confirm Restart?",
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HeartbeatPoint {
    status: String,
    latency_ms: Option<i64>,
    checked_at: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct IncidentFe {
    status: String,
    started_at: String,
    ended_at: Option<String>,
    duration_secs: i64,
    check_count: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct UptimeSummaryFe {
    uptime_24h: Option<f64>,
    uptime_7d: Option<f64>,
    uptime_30d: Option<f64>,
    heartbeats: Vec<HeartbeatPoint>,
    incidents: Vec<IncidentFe>,
}

async fn fetch_uptime_summary(id: i64) -> Option<UptimeSummaryFe> {
    let resp = gloo_net::http::Request::get(&format!("/api/v1/services/{id}/uptime"))
        .send()
        .await
        .ok()?;
    if !resp.ok() {
        return None;
    }
    resp.json::<UptimeSummaryFe>().await.ok()
}

/// Fires a start/stop/restart request and returns `Ok(())` or a
/// human-readable error extracted from the response body when possible.
async fn send_control(id: i64, action: ControlAction) -> Result<(), String> {
    let resp = gloo_net::http::Request::post(&format!("/api/v1/services/{id}/{}", action.as_str()))
        .send()
        .await
        .map_err(|_| "Could not reach the server.".to_string())?;
    if resp.ok() {
        return Ok(());
    }
    let msg = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Request failed.".to_string());
    Err(msg)
}

fn stat_text(v: Option<f64>) -> String {
    match v {
        Some(v) => format!("{v:.1}%"),
        None => "—".to_string(),
    }
}

fn format_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else if mins > 0 {
        format!("{mins}m")
    } else {
        format!("{secs}s")
    }
}

/// Shows uptime percentages, a heartbeat bar, and incident history for one
/// service, plus (admin only, when the service is backed by a systemd unit
/// or container) start/stop/restart controls. Opened by setting `target` to
/// `Some((service_id, display_name, controllable))`; closed by setting it
/// back to `None`.
#[component]
pub fn HistoryModal(target: RwSignal<Option<(i64, String, bool)>>) -> impl IntoView {
    let summary: RwSignal<Option<UptimeSummaryFe>> = RwSignal::new(None);
    let pending_confirm: RwSignal<Option<ControlAction>> = RwSignal::new(None);
    let control_busy = RwSignal::new(false);
    let control_msg: RwSignal<Option<(bool, String)>> = RwSignal::new(None);
    let logs_open = RwSignal::new(false);
    let log_lines: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let logs_panel_ref = NodeRef::<leptos::html::Div>::new();

    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    Effect::new(move |_| {
        if let Some((id, _, _)) = target.get() {
            summary.set(None);
            pending_confirm.set(None);
            control_msg.set(None);
            logs_open.set(false);
            spawn_local(async move {
                let fetched = fetch_uptime_summary(id).await;
                summary.set(fetched);
            });
        }
    });

    // Opens/closes the log-tail EventSource whenever `logs_open` or the target
    // service changes. `log_source` is captured once by this single long-lived
    // effect closure (not re-created per run), so it remembers the previous
    // connection across reactive updates. Tearing it down first — before
    // possibly reopening — covers "toggled off", "modal closed" (target ->
    // None), and a target change all in one place, so cleanup can't be missed.
    #[cfg(target_arch = "wasm32")]
    {
        use std::cell::RefCell;
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        use web_sys::EventSource;

        let log_source: RefCell<Option<EventSource>> = RefCell::new(None);

        Effect::new(move |_| {
            if let Some(es) = log_source.borrow_mut().take() {
                es.close();
            }

            if logs_open.get() {
                if let Some((id, _, _)) = target.get() {
                    log_lines.set(Vec::new());
                    if let Ok(es) = EventSource::new(&format!("/api/v1/services/{id}/logs/stream"))
                    {
                        let on_message =
                            Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                                if let Some(data) = event.data().as_string() {
                                    log_lines.update(|lines| {
                                        lines.push(data);
                                        if lines.len() > 500 {
                                            let excess = lines.len() - 500;
                                            lines.drain(0..excess);
                                        }
                                    });
                                    if let Some(el) = logs_panel_ref.get_untracked() {
                                        el.set_scroll_top(el.scroll_height());
                                    }
                                }
                            }) as Box<dyn FnMut(_)>);
                        es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                        on_message.forget();
                        *log_source.borrow_mut() = Some(es);
                    }
                }
            }
        });
    }

    let close = move || {
        target.set(None);
        summary.set(None);
        pending_confirm.set(None);
        control_msg.set(None);
        logs_open.set(false);
    };

    let fire_control = move |action: ControlAction| {
        let Some((id, _, _)) = target.get() else {
            return;
        };
        pending_confirm.set(None);
        control_msg.set(None);
        control_busy.set(true);
        spawn_local(async move {
            let result = send_control(id, action).await;
            control_busy.set(false);
            control_msg.set(Some(match result {
                Ok(()) => (false, format!("{} requested.", action.label())),
                Err(e) => (true, e),
            }));
            if let Some(fetched) = fetch_uptime_summary(id).await {
                summary.set(Some(fetched));
            }
        });
    };

    view! {
        <Show when=move || target.get().is_some()>
            <div style="position:fixed; inset:0; z-index:50; display:flex; align-items:center; justify-content:center;">
                // Backdrop
                <div
                    style="position:absolute; inset:0; background:rgba(0,0,0,0.6); backdrop-filter:blur(4px);"
                    on:click=move |_| close()
                ></div>
                // Panel
                <div style="position:relative; background:var(--color-bg-surface); border:1px solid var(--color-border); \
                             border-radius:1rem; box-shadow:0 25px 50px rgba(0,0,0,0.5); \
                             width:100%; max-width:520px; padding:1.5rem; margin:1rem; \
                             max-height:80vh; overflow-y:auto;">
                    <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:1.25rem;">
                        <h2 style="font-size:1rem; font-weight:600; margin:0;">
                            {move || target.get().map(|(_, name, _)| name).unwrap_or_default()}
                        </h2>
                        <button
                            style="background:none; border:none; cursor:pointer; color:var(--color-text-muted); \
                                   padding:0.25rem; border-radius:0.375rem; line-height:1;"
                            on:click=move |_| close()
                        >
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2"
                                 stroke-linecap="round" stroke-linejoin="round">
                                <line x1="18" y1="6" x2="6" y2="18"/>
                                <line x1="6" y1="6" x2="18" y2="18"/>
                            </svg>
                        </button>
                    </div>

                    <Show when=move || {
                        is_admin() && target.get().map(|(_, _, controllable)| controllable).unwrap_or(false)
                    }>
                        <div class="history-controls">
                            <button
                                class="btn-secondary"
                                disabled=move || control_busy.get()
                                on:click=move |_| fire_control(ControlAction::Start)
                            >
                                {ControlAction::Start.label()}
                            </button>
                            <button
                                class=move || if pending_confirm.get() == Some(ControlAction::Stop) { "btn-danger" } else { "btn-secondary" }
                                disabled=move || control_busy.get()
                                on:click=move |_| {
                                    if pending_confirm.get() == Some(ControlAction::Stop) {
                                        fire_control(ControlAction::Stop);
                                    } else {
                                        pending_confirm.set(Some(ControlAction::Stop));
                                    }
                                }
                            >
                                {move || if pending_confirm.get() == Some(ControlAction::Stop) {
                                    ControlAction::Stop.confirm_label()
                                } else {
                                    ControlAction::Stop.label()
                                }}
                            </button>
                            <button
                                class=move || if pending_confirm.get() == Some(ControlAction::Restart) { "btn-danger" } else { "btn-secondary" }
                                disabled=move || control_busy.get()
                                on:click=move |_| {
                                    if pending_confirm.get() == Some(ControlAction::Restart) {
                                        fire_control(ControlAction::Restart);
                                    } else {
                                        pending_confirm.set(Some(ControlAction::Restart));
                                    }
                                }
                            >
                                {move || if pending_confirm.get() == Some(ControlAction::Restart) {
                                    ControlAction::Restart.confirm_label()
                                } else {
                                    ControlAction::Restart.label()
                                }}
                            </button>
                        </div>
                        {move || control_msg.get().map(|(is_err, msg)| view! {
                            <p
                                class="history-control-msg"
                                style=move || format!(
                                    "color:{}",
                                    if is_err { "var(--color-danger)" } else { "var(--color-success)" }
                                )
                            >
                                {msg}
                            </p>
                        })}
                    </Show>

                    <Show when=move || {
                        is_admin() && target.get().map(|(_, _, controllable)| controllable).unwrap_or(false)
                    }>
                        <button
                            class="btn-secondary settings-btn-sm"
                            style="margin-bottom:0.75rem;"
                            on:click=move |_| logs_open.update(|v| *v = !*v)
                        >
                            {move || if logs_open.get() { "Hide Logs" } else { "Show Logs" }}
                        </button>
                        <Show when=move || logs_open.get()>
                            <div class="history-logs" node_ref=logs_panel_ref>
                                {move || log_lines.get().iter().map(|line| view! {
                                    <div class="history-log-line">{line.clone()}</div>
                                }).collect_view()}
                            </div>
                        </Show>
                    </Show>

                    {move || match summary.get() {
                        None => view! {
                            <p class="text-xs" style="color:var(--color-text-muted);">"Loading…"</p>
                        }.into_any(),
                        Some(s) => view! {
                            <div>
                                <div class="history-stats">
                                    <div class="history-stat">
                                        <p class="history-stat-value">{stat_text(s.uptime_24h)}</p>
                                        <p class="history-stat-label">"24h"</p>
                                    </div>
                                    <div class="history-stat">
                                        <p class="history-stat-value">{stat_text(s.uptime_7d)}</p>
                                        <p class="history-stat-label">"7d"</p>
                                    </div>
                                    <div class="history-stat">
                                        <p class="history-stat-value">{stat_text(s.uptime_30d)}</p>
                                        <p class="history-stat-label">"30d"</p>
                                    </div>
                                </div>

                                {(!s.heartbeats.is_empty()).then(|| view! {
                                    <div class="history-heartbeat">
                                        {s.heartbeats.iter().map(|h| {
                                            let color = match h.status.as_str() {
                                                "up" => "var(--color-success)",
                                                "down" => "var(--color-danger)",
                                                _ => "var(--color-text-muted)",
                                            };
                                            let latency = h.latency_ms.map(|ms| format!(" ({ms}ms)")).unwrap_or_default();
                                            let when = h.checked_at.clone().unwrap_or_default();
                                            let tooltip = format!("{}{latency} — {when}", h.status);
                                            view! {
                                                <span class="history-heartbeat-bar" style=format!("background:{color};") title=tooltip></span>
                                            }
                                        }).collect_view()}
                                    </div>
                                })}

                                <p class="history-incidents-title">"Incidents"</p>
                                {if s.incidents.is_empty() {
                                    view! {
                                        <p class="text-xs" style="color:var(--color-text-muted);">
                                            "No incidents in the retained history."
                                        </p>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="history-incidents">
                                            {s.incidents.iter().map(|inc| {
                                                let color = if inc.status == "unknown" {
                                                    "var(--color-text-muted)"
                                                } else {
                                                    "var(--color-danger)"
                                                };
                                                let range = match &inc.ended_at {
                                                    Some(end) => format!("{} → {}", inc.started_at, end),
                                                    None => format!("{} → ongoing", inc.started_at),
                                                };
                                                let checks = inc.check_count;
                                                let meta = format!(
                                                    "{} · {checks} check{}",
                                                    format_duration(inc.duration_secs),
                                                    if checks == 1 { "" } else { "s" },
                                                );
                                                view! {
                                                    <div class="history-incident-row">
                                                        <span class="history-incident-dot" style=format!("background:{color};")></span>
                                                        <div>
                                                            <p class="history-incident-range">{range}</p>
                                                            <p class="history-incident-meta">{meta}</p>
                                                        </div>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        }.into_any(),
                    }}
                </div>
            </div>
        </Show>
    }
}
