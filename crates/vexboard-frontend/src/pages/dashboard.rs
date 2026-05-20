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
            <div class="flex items-center justify-between mb-6">
                <h1 class="text-xl font-semibold">"Services"</h1>
                <button class="btn-primary">"+ Add Service"</button>
            </div>

            <Suspense fallback=move || view! { <p class="text-[var(--color-text-muted)]">"Loading services..."</p> }>
                {move || services.get().map(|svcs| {
                    if svcs.is_empty() {
                        Either::Left(view! {
                            <div class="text-center py-12">
                                <p class="text-[var(--color-text-muted)]">"No services configured yet."</p>
                                <p class="text-xs text-[var(--color-text-muted)] mt-1">"Add one manually or discover running systemd services."</p>
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
