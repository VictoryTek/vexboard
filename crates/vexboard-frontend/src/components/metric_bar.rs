use leptos::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SystemMetrics {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_used_kb: u64,
    pub memory_total_kb: u64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
}

#[component]
pub fn MetricBar() -> impl IntoView {
    let (metrics, set_metrics) = create_signal(SystemMetrics::default());

    // Connect to SSE stream for live metrics
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        use web_sys::EventSource;

        create_effect(move |_| {
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

    let format_bytes = |bytes: u64| -> String {
        if bytes > 1_073_741_824 {
            format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
        } else if bytes > 1_048_576 {
            format!("{:.1} MB", bytes as f64 / 1_048_576.0)
        } else if bytes > 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{bytes} B")
        }
    };

    view! {
        <div class="h-12 border-b border-[var(--color-border)] bg-[var(--color-bg-surface)] px-6 flex items-center gap-6">
            <div class="flex items-center gap-2">
                <span class="text-xs text-[var(--color-text-muted)]">"CPU"</span>
                <span class="metric-value text-sm">
                    {move || format!("{:.1}%", metrics.get().cpu_percent)}
                </span>
            </div>
            <div class="flex items-center gap-2">
                <span class="text-xs text-[var(--color-text-muted)]">"RAM"</span>
                <span class="metric-value text-sm">
                    {move || format!("{:.1}%", metrics.get().memory_percent)}
                </span>
            </div>
            <div class="flex items-center gap-2">
                <span class="text-xs text-[var(--color-text-muted)]">"NET ↓"</span>
                <span class="metric-value text-sm">
                    {move || format_bytes(metrics.get().net_rx_bytes)}
                </span>
            </div>
            <div class="flex items-center gap-2">
                <span class="text-xs text-[var(--color-text-muted)]">"NET ↑"</span>
                <span class="metric-value text-sm">
                    {move || format_bytes(metrics.get().net_tx_bytes)}
                </span>
            </div>
        </div>
    }
}
