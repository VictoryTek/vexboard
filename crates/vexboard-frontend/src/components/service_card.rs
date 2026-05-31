use leptos::prelude::*;

use crate::components::status_badge::StatusDot;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ServiceData {
    pub id: i64,
    pub systemd_unit: Option<String>,
    pub discovery_source: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
}

#[component]
pub fn ServiceCard(service: ServiceData, #[prop(into)] on_delete: Callback<i64>) -> impl IntoView {
    let service_id = service.id;
    let (badge_cls, status_label) = match service.status.as_str() {
        "up" => ("status-badge status-badge-up", "Up"),
        "down" => ("status-badge status-badge-down", "Down"),
        _ => ("status-badge status-badge-unknown", "—"),
    };

    let latency = service.latency_ms.map(|ms| format!("{ms}ms"));

    let first = service.display_name.chars().next().unwrap_or('?');
    let letter = first.to_ascii_uppercase().to_string();
    let icon_opt = service.icon.clone().filter(|i| !i.is_empty());
    let is_url_icon = icon_opt.as_ref().map_or(false, |i| {
        i.starts_with("http://") || i.starts_with("https://")
    });
    let icon_text = if is_url_icon {
        letter.clone()
    } else {
        icon_opt.clone().unwrap_or(letter)
    };
    let icon_url = if is_url_icon { icon_opt } else { None };

    let normalized_source = service
        .discovery_source
        .as_ref()
        .map(|s| s.trim().to_ascii_lowercase());

    let source_badge = if let Some(src) = normalized_source.as_deref() {
        match src {
            "docker" => Some(("Docker".to_string(), "#0db7ed".to_string())),
            "podman" => Some(("Podman".to_string(), "#892ca0".to_string())),
            "systemd" => Some(("Systemd".to_string(), "var(--color-accent)".to_string())),
            _ => None,
        }
    } else if service
        .systemd_unit
        .as_deref()
        .map(|u| u.ends_with(".service"))
        .unwrap_or(false)
    {
        Some(("Systemd".to_string(), "var(--color-accent)".to_string()))
    } else {
        None
    };

    let summary = service
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .or_else(|| service.url.clone());

    view! {
        <div class="service-card" style="position:relative;">
            {source_badge.clone().map(|(label, color)| view! {
                <span style=format!(
                    "position:absolute; top:0.65rem; right:0.65rem; display:inline-block; \
                     font-size:0.6rem; font-weight:700; letter-spacing:0.06em; \
                     text-transform:uppercase; color:{color}; background:{color}22; \
                     border-radius:0.25rem; padding:0.1rem 0.4rem;"
                )>
                    {label}
                </span>
            })}

            <div style="display:flex; align-items:flex-start; justify-content:space-between; gap:0.75rem; padding-right:0.1rem;">
                <div style="display:flex; align-items:flex-start; gap:0.75rem; min-width:0; flex:1;">
                    <div class="service-icon" style="position:relative; margin-top:0.05rem;">
                        <span>{icon_text}</span>
                        {icon_url.map(|src| view! {
                            <img src={src} alt=""
                                style="position:absolute;top:0;left:0;width:100%;height:100%;object-fit:contain;border-radius:inherit;padding:3px;"
                                on:error=move |ev| {
                                    use wasm_bindgen::JsCast;
                                    if let Some(t) = ev.target() {
                                        if let Ok(el) = t.dyn_into::<web_sys::HtmlElement>() {
                                            let _ = el.style().set_property("display", "none");
                                        }
                                    }
                                }
                            />
                        })}
                    </div>
                    <div style="min-width:0; flex:1; padding-right:0.25rem;">
                        <p style="font-size:1rem; font-weight:600; line-height:1.15; color:var(--color-text-primary); margin:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                            {service.display_name}
                        </p>
                        {summary.map(|d| view! {
                            <p style="font-size:0.8rem; line-height:1.25; color:var(--color-text-secondary); margin:0.2rem 0 0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                                {d}
                            </p>
                        })}
                    </div>
                </div>

                <div style="display:flex; flex-direction:column; align-items:flex-end; gap:0.35rem; flex-shrink:0; margin-top:0.2rem;">
                    <div class={badge_cls}>
                        <StatusDot status=service.status.clone()/>
                        <span>{status_label}</span>
                        {latency.map(|lat| view! {
                            <span style="font-size:0.65rem;font-weight:400;opacity:0.65;text-transform:none;letter-spacing:0">
                                {lat}
                            </span>
                        })}
                    </div>
                </div>
            </div>

            // URL footer
            {service.url.as_ref().map(|url| view! {
                <div class="mt-3 pt-3" style="border-top: 1px solid var(--color-border)">
                    <a
                        href={url.clone()}
                        target="_blank"
                        rel="noopener noreferrer"
                        class="flex items-center gap-1.5 text-xs transition-colors"
                        style="color: var(--color-text-muted)"
                        onmouseover="this.style.color='var(--color-accent)'"
                        onmouseout="this.style.color='var(--color-text-muted)'"
                    >
                        <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2"
                             stroke-linecap="round" stroke-linejoin="round"
                             style="flex-shrink:0">
                            <circle cx="12" cy="12" r="10"/>
                            <line x1="2" y1="12" x2="22" y2="12"/>
                            <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
                        </svg>
                        <span class="truncate">{url.clone()}</span>
                    </a>
                </div>
            })}

            // Delete action
            <div style="display:flex; justify-content:flex-end; margin-top:0.4rem;">
                <button
                    style="background:none; border:none; cursor:pointer; \
                           color:var(--color-text-muted); opacity:0.35; padding:0.15rem 0; \
                           font-size:0.7rem; display:flex; align-items:center; gap:0.25rem; \
                           line-height:1;"
                    onmouseover="this.style.opacity='1'; this.style.color='var(--color-danger)'"
                    onmouseout="this.style.opacity='0.35'; this.style.color='var(--color-text-muted)'"
                    on:click=move |_| on_delete.run(service_id)
                >
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2"
                         stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="3 6 5 6 21 6"/>
                        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                    </svg>
                    "Remove"
                </button>
            </div>
        </div>
    }
}
