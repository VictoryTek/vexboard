use leptos::either::Either;
use leptos::prelude::*;

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

    view! {
        <div>
            <div class="page-header">
                <h1 class="page-title">"Services"</h1>
                <button class="btn-primary">
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
                                    <p class="text-sm font-semibold"
                                       style="color: var(--color-text-secondary)">
                                        "No services configured"
                                    </p>
                                    <p class="text-xs mt-1" style="color: var(--color-text-muted)">
                                        "Add a service or discover running systemd units."
                                    </p>
                                </div>
                                <button class="btn-primary">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.5"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <line x1="12" y1="5" x2="12" y2="19"/>
                                        <line x1="5" y1="12" x2="19" y2="12"/>
                                    </svg>
                                    "Add your first service"
                                </button>
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
