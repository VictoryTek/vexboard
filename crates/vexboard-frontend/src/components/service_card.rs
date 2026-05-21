use leptos::prelude::*;

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
    let (badge_cls, dot_cls, status_label) = match service.status.as_str() {
        "up"   => ("status-badge status-badge-up",      "status-dot status-dot-up",      "Up"),
        "down" => ("status-badge status-badge-down",    "status-dot status-dot-down",    "Down"),
        _      => ("status-badge status-badge-unknown", "status-dot status-dot-unknown", "—"),
    };

    let latency = service.latency_ms.map(|ms| format!("{ms}ms"));

    let first = service.display_name.chars().next().unwrap_or('?');
    let icon_display = service
        .icon
        .clone()
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| first.to_ascii_uppercase().to_string());

    view! {
        <div class="service-card">
            // Header: icon + name/description + status badge
            <div class="flex items-start justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                    <div class="service-icon">
                        <span>{icon_display}</span>
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
                    <span class={dot_cls}></span>
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
