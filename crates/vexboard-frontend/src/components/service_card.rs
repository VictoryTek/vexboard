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
    let status_class = match service.status.as_str() {
        "up" => "badge-up",
        "down" => "badge-down",
        _ => "badge-unknown",
    };

    let latency_text = service
        .latency_ms
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_else(|| "—".to_string());

    view! {
        <div class="card hover:bg-[var(--color-bg-hover)] transition-colors cursor-pointer group">
            <div class="flex items-start justify-between">
                <div class="flex items-center gap-3">
                    <div class="w-10 h-10 rounded-lg bg-[var(--color-bg-hover)] flex items-center justify-center text-[var(--color-text-secondary)]">
                        {service.icon.clone().unwrap_or_else(|| "●".to_string())}
                    </div>
                    <div>
                        <h3 class="font-medium text-sm">{service.display_name.clone()}</h3>
                        {service.description.as_ref().map(|d| view! {
                            <p class="text-xs text-[var(--color-text-muted)] mt-0.5">{d.clone()}</p>
                        })}
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    <span class={status_class}>
                        <super::status_badge::StatusDot status=service.status.clone() />
                        {latency_text}
                    </span>
                </div>
            </div>
            {service.url.as_ref().map(|url| view! {
                <div class="mt-3 pt-3 border-t border-[var(--color-border)]">
                    <a
                        href={url.clone()}
                        target="_blank"
                        rel="noopener noreferrer"
                        class="text-xs text-[var(--color-accent)] hover:underline"
                    >
                        {url.clone()}
                    </a>
                </div>
            })}
        </div>
    }
}
