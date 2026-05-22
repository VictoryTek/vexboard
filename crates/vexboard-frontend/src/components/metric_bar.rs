use leptos::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct SystemMetrics {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_used_kb: u64,
    pub memory_total_kb: u64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
}

fn format_bytes(bytes: u64) -> String {
    if bytes > 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes > 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes > 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn usage_color(pct: f64) -> &'static str {
    if pct > 85.0 {
        "var(--color-danger)"
    } else if pct > 60.0 {
        "var(--color-warning)"
    } else {
        "var(--color-text-primary)"
    }
}

#[component]
pub fn MetricBar() -> impl IntoView {
    let (metrics, set_metrics) = signal(SystemMetrics::default());

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        use web_sys::EventSource;

        Effect::new(move |_| {
            let es = EventSource::new("/api/v1/metrics/stream").ok();
            if let Some(es) = es {
                let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                    if let Some(data) = event.data().as_string() {
                        if let Ok(snapshot) = serde_json::from_str::<SystemMetrics>(&data) {
                            set_metrics.set(snapshot);
                        }
                    }
                }) as Box<dyn FnMut(_)>);

                es.add_event_listener_with_callback("system", on_message.as_ref().unchecked_ref())
                    .ok();
                on_message.forget();
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_metrics;

    view! {
        <div class="metric-bar">
            // CPU
            <div class="metric-item">
                <span class="metric-label">"CPU"</span>
                <span class="metric-val"
                    style=move || format!("color: {}", usage_color(metrics.get().cpu_percent))>
                    {move || format!("{:.1}%", metrics.get().cpu_percent)}
                </span>
            </div>

            <div class="metric-sep"></div>

            // RAM
            <div class="metric-item">
                <span class="metric-label">"RAM"</span>
                <span class="metric-val"
                    style=move || format!("color: {}", usage_color(metrics.get().memory_percent))>
                    {move || format!("{:.1}%", metrics.get().memory_percent)}
                </span>
            </div>

            <div class="metric-sep"></div>

            // Network in
            <div class="metric-item">
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                     stroke="var(--color-text-muted)" stroke-width="2.5"
                     stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="19" x2="12" y2="5"/>
                    <polyline points="5 12 12 19 19 12"/>
                </svg>
                <span class="metric-label">"IN"</span>
                <span class="metric-val">{move || format_bytes(metrics.get().net_rx_bytes)}</span>
            </div>

            <div class="metric-sep"></div>

            // Network out
            <div class="metric-item">
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                     stroke="var(--color-text-muted)" stroke-width="2.5"
                     stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="5" x2="12" y2="19"/>
                    <polyline points="19 12 12 5 5 12"/>
                </svg>
                <span class="metric-label">"OUT"</span>
                <span class="metric-val">{move || format_bytes(metrics.get().net_tx_bytes)}</span>
            </div>
        </div>
    }
}
