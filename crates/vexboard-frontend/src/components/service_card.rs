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
pub fn ServiceCard(
    service: ServiceData,
    on_delete: Option<Callback<i64>>,
    on_edit: Option<Callback<i64>>,
) -> impl IntoView {
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
    let is_url_icon = icon_opt
        .as_ref()
        .is_some_and(|i| i.starts_with("http://") || i.starts_with("https://"));
    let icon_text = if is_url_icon {
        letter.clone()
    } else {
        icon_opt.clone().unwrap_or(letter)
    };
    let icon_url = if is_url_icon { icon_opt } else { None };
    let img_failed = RwSignal::new(false);

    let normalized_source = service
        .discovery_source
        .as_ref()
        .map(|s| s.trim().to_ascii_lowercase());

    let source_badge = if let Some(src) = normalized_source.as_deref() {
        match src {
            "docker" => Some(("Docker".to_string(), "#0db7ed".to_string())),
            "podman" => Some(("Podman".to_string(), "#892ca0".to_string())),
            "systemd" => Some(("Systemd".to_string(), "#e8873a".to_string())),
            _ => None,
        }
    } else if service
        .systemd_unit
        .as_deref()
        .map(|u| u.ends_with(".service"))
        .unwrap_or(false)
    {
        Some(("Systemd".to_string(), "#e8873a".to_string()))
    } else {
        None
    };

    let summary = service
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .or_else(|| service.url.clone());

    let url_href = service.url.clone().unwrap_or_default();
    let has_url = service.url.is_some();
    let card_style = if has_url {
        "display:block; text-decoration:none; cursor:pointer;"
    } else {
        "display:block; text-decoration:none; cursor:default;"
    };

    view! {
        <a
            href={url_href}
            target={if has_url { "_blank" } else { "_self" }}
            rel="noopener noreferrer"
            class="service-card"
            style={card_style}
            on:click=move |ev| { if !has_url { ev.prevent_default(); } }
        >
            // Top row: icon + title (left) | source badge (right)
            <div style="display:flex; align-items:center; justify-content:space-between; gap:0.75rem; margin-bottom:0.35rem;">
                <div style="display:flex; align-items:center; gap:0.75rem; min-width:0; flex:1;">
                    <div class="service-icon" style="flex-shrink:0;">
                        {move || match (icon_url.clone(), img_failed.get()) {
                            (Some(src), false) => view! {
                                <img src={src} alt=""
                                    style="width:100%;height:100%;object-fit:contain;border-radius:inherit;padding:3px;"
                                    on:error=move |_| img_failed.set(true)
                                />
                            }.into_any(),
                            _ => view! { <span>{icon_text.clone()}</span> }.into_any(),
                        }}
                    </div>
                    <p style="font-size:1rem; font-weight:600; line-height:1.15; color:var(--color-text-primary); margin:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                        {service.display_name}
                    </p>
                </div>

                {source_badge.clone().map(|(label, color)| view! {
                    <span style=format!(
                        "flex-shrink:0; display:inline-flex; align-items:center; \
                         font-size:0.68rem; font-weight:700; letter-spacing:0.04em; \
                         text-transform:uppercase; color:{color}; background:{color}22; \
                         border:1px solid {color}40; border-radius:20px; padding:3px 9px;"
                    )>
                        {label}
                    </span>
                })}
            </div>

            // Description row
            {summary.map(|d| view! {
                <p style="font-size:0.8rem; line-height:1.25; color:var(--color-text-secondary); margin:0 0 0.1rem; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                    {d}
                </p>
            })}

            // Bottom row: status badge (left) | edit + remove (right, admin only)
            <div style="display:flex; align-items:center; justify-content:space-between; margin-top:0.4rem;"
                on:click=move |ev| { ev.prevent_default(); ev.stop_propagation(); }
            >
                <div class={badge_cls}>
                    <StatusDot status=service.status.clone()/>
                    <span>{status_label}</span>
                    {latency.map(|lat| view! {
                        <span style="font-size:0.65rem;font-weight:400;opacity:0.65;text-transform:none;letter-spacing:0">
                            {lat}
                        </span>
                    })}
                </div>
                <div style="display:flex; align-items:center; gap:0.75rem;">
                    {on_edit.map(|cb| view! {
                        <button
                            style="background:none; border:none; cursor:pointer; \
                                   color:var(--color-text-muted); opacity:0.35; padding:0.15rem 0; \
                                   font-size:0.7rem; display:flex; align-items:center; gap:0.25rem; \
                                   line-height:1;"
                            onmouseover="this.style.opacity='1'; this.style.color='var(--color-accent)'"
                            onmouseout="this.style.opacity='0.35'; this.style.color='var(--color-text-muted)'"
                            on:click=move |_| cb.run(service_id)
                        >
                            <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2"
                                 stroke-linecap="round" stroke-linejoin="round">
                                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                            </svg>
                            "Edit"
                        </button>
                    })}
                    {on_delete.map(|cb| view! {
                        <button
                            style="background:none; border:none; cursor:pointer; \
                                   color:var(--color-text-muted); opacity:0.35; padding:0.15rem 0; \
                                   font-size:0.7rem; display:flex; align-items:center; gap:0.25rem; \
                                   line-height:1;"
                            onmouseover="this.style.opacity='1'; this.style.color='var(--color-danger)'"
                            onmouseout="this.style.opacity='0.35'; this.style.color='var(--color-text-muted)'"
                            on:click=move |_| cb.run(service_id)
                        >
                            <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2"
                                 stroke-linecap="round" stroke-linejoin="round">
                                <polyline points="3 6 5 6 21 6"/>
                                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                            </svg>
                            "Remove"
                        </button>
                    })}
                </div>
            </div>
        </a>
    }
}
