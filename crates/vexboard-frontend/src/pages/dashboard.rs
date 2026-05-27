use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::discovery_panel::DiscoveryPanel;
use crate::components::modal_edit::{EditFormData, EditModal};
use crate::components::service_card::{ServiceCard, ServiceData};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ServiceResponse {
    id: i64,
    display_name: String,
    description: Option<String>,
    url: Option<String>,
    icon: Option<String>,
    status: String,
    latency_ms: Option<i64>,
}

#[component]
pub fn DashboardPage() -> impl IntoView {
    let services = LocalResource::new(|| async move { fetch_services().await.unwrap_or_default() });
    let (show_modal, set_show_modal) = signal(false);

    let on_save = Callback::new(move |data: EditFormData| {
        spawn_local(async move {
            let body = serde_json::json!({
                "display_name": data.display_name,
                "description": if data.description.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.description) },
                "url": if data.url.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.url) },
                "icon": if data.icon.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.icon) },
                "probe_enabled": data.probe_enabled,
                "probe_interval": data.probe_interval,
            });
            if let Ok(req) = gloo_net::http::Request::post("/api/v1/services").json(&body) {
                let _ = req.send().await;
            }
            set_show_modal.set(false);
            services.refetch();
        });
    });

    view! {
        <EditModal
            visible=show_modal
            on_close=Callback::new(move |_| set_show_modal.set(false))
            on_save=on_save
        />
        <div>
            <div class="page-header">
                <h1 class="page-title">"Services"</h1>
                <button class="btn-primary" on:click=move |_| set_show_modal.set(true)>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5"
                         stroke-linecap="round" stroke-linejoin="round">
                        <line x1="12" y1="5" x2="12" y2="19"/>
                        <line x1="5" y1="12" x2="19" y2="12"/>
                    </svg>
                    "Add Service"
                </button>
            </div>

            <Suspense fallback=move || view! {
                <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {(0..3_u8).map(|_| view! {
                        <div class="service-card" style="opacity:0.35;pointer-events:none">
                            <div class="flex items-start gap-3">
                                <div class="service-icon" style="background:var(--color-bg-hover);border-color:transparent"></div>
                                <div class="space-y-2 flex-1">
                                    <div style="width:120px;height:12px;border-radius:6px;background:var(--color-bg-hover)"></div>
                                    <div style="width:80px;height:10px;border-radius:6px;background:var(--color-bg-hover)"></div>
                                </div>
                            </div>
                        </div>
                    }).collect_view()}
                </div>
            }>
                {move || services.get().map(|svcs| {
                    if svcs.is_empty() {
                        Either::Left(view! {
                            <div class="empty-state">
                                <div class="empty-icon">
                                    <svg width="26" height="26" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="1.5"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <rect x="2" y="3" width="20" height="14" rx="2"/>
                                        <line x1="8" y1="21" x2="16" y2="21"/>
                                        <line x1="12" y1="17" x2="12" y2="21"/>
                                    </svg>
                                </div>
                                <div>
                                    <p style="font-size:0.875rem; font-weight:600; color:var(--color-text-secondary);">
                                        "No services configured"
                                    </p>
                                    <p style="font-size:0.75rem; margin-top:0.25rem; color:var(--color-text-muted);">
                                        "Use \"+ Add Service\" above to get started."
                                    </p>
                                </div>
                            </div>
                        })
                    } else {
                        Either::Right(view! {
                            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                                {svcs.into_iter().map(|svc| {
                                    let data = ServiceData {
                                        id: svc.id,
                                        display_name: svc.display_name,
                                        description: svc.description,
                                        url: svc.url,
                                        icon: svc.icon,
                                        status: svc.status,
                                        latency_ms: svc.latency_ms,
                                    };
                                    view! { <ServiceCard service=data /> }
                                }).collect_view()}
                            </div>
                        })
                    }
                })}
            </Suspense>
            <DiscoveryPanel on_added=Callback::new(move |_| services.refetch()) />
        </div>
    }
}

async fn fetch_services() -> Result<Vec<ServiceResponse>, gloo_net::Error> {
    let resp = gloo_net::http::Request::get("/api/v1/services")
        .send()
        .await?;
    let services: Vec<ServiceResponse> = resp.json().await?;
    Ok(services)
}
