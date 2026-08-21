use leptos::prelude::*;
use leptos::task::spawn_local;

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
/// service. Opened by setting `target` to `Some((service_id, display_name))`;
/// closed by setting it back to `None`.
#[component]
pub fn HistoryModal(target: RwSignal<Option<(i64, String)>>) -> impl IntoView {
    let summary: RwSignal<Option<UptimeSummaryFe>> = RwSignal::new(None);

    Effect::new(move |_| {
        if let Some((id, _)) = target.get() {
            summary.set(None);
            spawn_local(async move {
                let fetched = fetch_uptime_summary(id).await;
                summary.set(fetched);
            });
        }
    });

    let close = move || {
        target.set(None);
        summary.set(None);
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
                            {move || target.get().map(|(_, name)| name).unwrap_or_default()}
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
