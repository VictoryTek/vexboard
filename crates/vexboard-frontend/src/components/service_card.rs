use leptos::either::Either;
use leptos::prelude::*;

use crate::components::status_badge::StatusDot;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ServiceData {
    pub id: i64,
    pub display_name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
}

#[component]
pub fn ServiceCard(service: ServiceData) -> impl IntoView {
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
        .map_or(false, |i| i.starts_with("http://") || i.starts_with("https://"));
    let icon_text = if is_url_icon {
        letter.clone()
    } else {
        icon_opt.clone().unwrap_or(letter)
    };
    let icon_url = if is_url_icon { icon_opt } else { None };

    view! {
        <div class="service-card">
            // Header: icon + name/description + status badge
            <div class="flex items-start justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                    <div class="service-icon" style="position:relative;">
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
                    <div class="min-w-0 flex-1">
                        <h3 class="text-sm font-semibold truncate leading-tight"
                            style="color: var(--color-text-primary)">
                            {service.display_name}
                        </h3>
                        {service.description.as_ref().map(|d| view! {
                            <p class="text-xs truncate mt-0.5 leading-snug"
                               style="color: var(--color-text-muted)">
                                {d.clone()}
                            </p>
                        })}
                    </div>
                </div>

                // Status badge
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
        </div>
    }
}
