use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::modal_edit::{EditFormData, EditModal};
use crate::components::quick_link_card::{QuickLinkCard, QuickLinkData};
use crate::components::quick_link_modal::{QuickLinkFormData, QuickLinkModal};
use crate::components::service_card::{ServiceCard, ServiceData};

#[derive(Debug, Clone, Copy, PartialEq)]
enum SortMode {
    Default,
    Alphabetical,
    Source,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ServiceResponse {
    id: i64,
    systemd_unit: Option<String>,
    discovery_source: Option<String>,
    display_name: String,
    description: Option<String>,
    url: Option<String>,
    icon: Option<String>,
    status: String,
    latency_ms: Option<i64>,
    probe_enabled: bool,
    probe_interval: i64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct QuickLinkResponse {
    id: i64,
    title: String,
    url: String,
    icon: Option<String>,
    description: Option<String>,
}

#[component]
pub fn DashboardPage() -> impl IntoView {
    let services = LocalResource::new(|| async move { fetch_services().await.unwrap_or_default() });
    let quick_links =
        LocalResource::new(|| async move { fetch_quick_links().await.unwrap_or_default() });

    let (show_modal, set_show_modal) = signal(false);
    let (show_add_menu, set_show_add_menu) = signal(false);
    let (sort_mode, set_sort_mode) = signal(SortMode::Default);

    // Edit targets
    let edit_target: RwSignal<Option<(i64, EditFormData)>> = RwSignal::new(None);
    let edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>> = RwSignal::new(None);
    let (show_add_link_modal, set_show_add_link_modal) = signal(false);

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

    let on_save_link = Callback::new(move |data: QuickLinkFormData| {
        spawn_local(async move {
            let body = serde_json::json!({
                "title": data.title,
                "url": data.url,
                "icon": if data.icon.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.icon) },
                "description": if data.description.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.description) },
            });
            if let Ok(req) = gloo_net::http::Request::post("/api/v1/quick-links").json(&body) {
                let _ = req.send().await;
            }
            set_show_add_link_modal.set(false);
            quick_links.refetch();
        });
    });

    view! {
        // Add service modal
        <EditModal
            visible=show_modal
            on_close=Callback::new(move |_| set_show_modal.set(false))
            on_save=on_save
        />

        // Add quick link modal
        <QuickLinkModal
            visible=show_add_link_modal
            on_close=Callback::new(move |_| set_show_add_link_modal.set(false))
            on_save=on_save_link
        />

        // Edit service modal
        {move || edit_target.get().map(|(id, initial)| {
            let (show_edit, set_show_edit) = signal(true);
            let on_edit_save = Callback::new(move |data: EditFormData| {
                spawn_local(async move {
                    let body = serde_json::json!({
                        "display_name": data.display_name,
                        "description": data.description,
                        "url": data.url,
                        "icon": data.icon,
                        "probe_enabled": data.probe_enabled,
                        "probe_interval": data.probe_interval,
                    });
                    if let Ok(req) = gloo_net::http::Request::put(&format!("/api/v1/services/{id}")).json(&body) {
                        let _ = req.send().await;
                    }
                    edit_target.set(None);
                    services.refetch();
                });
            });
            view! {
                <EditModal
                    visible=show_edit
                    title="Edit Service"
                    initial=initial
                    on_close=Callback::new(move |_| { set_show_edit.set(false); edit_target.set(None); })
                    on_save=on_edit_save
                />
            }
        })}

        // Edit quick link modal
        {move || edit_link_target.get().map(|(id, initial)| {
            let (show_edit, set_show_edit) = signal(true);
            let on_edit_save = Callback::new(move |data: QuickLinkFormData| {
                spawn_local(async move {
                    let body = serde_json::json!({
                        "title": data.title,
                        "url": data.url,
                        "icon": data.icon,
                        "description": data.description,
                    });
                    if let Ok(req) = gloo_net::http::Request::put(&format!("/api/v1/quick-links/{id}")).json(&body) {
                        let _ = req.send().await;
                    }
                    edit_link_target.set(None);
                    quick_links.refetch();
                });
            });
            view! {
                <QuickLinkModal
                    visible=show_edit
                    title="Edit Quick Link"
                    initial=initial
                    on_close=Callback::new(move |_| { set_show_edit.set(false); edit_link_target.set(None); })
                    on_save=on_edit_save
                />
            }
        })}

        <div>
            // ── Services section ──────────────────────────────────────────────────
            <div class="page-header">
                <h1 class="page-title">"Services"</h1>
                <div style="display:flex; align-items:center; gap:0.5rem;">
                    // Sort controls
                    <div style="display:flex; align-items:center; gap:0.25rem; \
                                background:var(--color-bg-surface); border:1px solid var(--color-border); \
                                border-radius:0.5rem; padding:0.2rem;">
                        {[
                            (SortMode::Default, "Default"),
                            (SortMode::Alphabetical, "A–Z"),
                            (SortMode::Source, "Source"),
                        ].map(|(mode, label)| view! {
                            <button
                                style=move || {
                                    let active = sort_mode.get() == mode;
                                    format!(
                                        "background:{}; color:{}; border:none; cursor:pointer; \
                                         border-radius:0.35rem; padding:0.2rem 0.6rem; \
                                         font-size:0.72rem; font-weight:{}; transition:all 0.15s;",
                                        if active { "var(--color-accent)" } else { "transparent" },
                                        if active { "#fff" } else { "var(--color-text-secondary)" },
                                        if active { "600" } else { "400" },
                                    )
                                }
                                on:click=move |_| set_sort_mode.set(mode)
                            >
                                {label}
                            </button>
                        })}
                    </div>

                    // "+ Add" dropdown
                    <div style="position:relative;">
                        <button class="btn-primary"
                            on:click=move |_| set_show_add_menu.update(|v| *v = !*v)
                        >
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5"
                                 stroke-linecap="round" stroke-linejoin="round">
                                <line x1="12" y1="5" x2="12" y2="19"/>
                                <line x1="5" y1="12" x2="19" y2="12"/>
                            </svg>
                            "Add"
                            <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5"
                                 stroke-linecap="round" stroke-linejoin="round"
                                 style="margin-left:0.1rem;">
                                <polyline points="6 9 12 15 18 9"/>
                            </svg>
                        </button>
                        <Show when=move || show_add_menu.get()>
                            // Backdrop to close menu on outside click
                            <div
                                style="position:fixed; inset:0; z-index:40;"
                                on:click=move |_| set_show_add_menu.set(false)
                            ></div>
                            <div style="position:absolute; right:0; top:calc(100% + 0.35rem); z-index:50; \
                                         background:var(--color-bg-surface); border:1px solid var(--color-border); \
                                         border-radius:0.6rem; box-shadow:0 8px 24px rgba(0,0,0,0.35); \
                                         min-width:160px; padding:0.3rem; overflow:hidden;">
                                <button
                                    style="width:100%; background:none; border:none; cursor:pointer; \
                                           display:flex; align-items:center; gap:0.6rem; \
                                           padding:0.5rem 0.75rem; border-radius:0.4rem; \
                                           font-size:0.82rem; color:var(--color-text-primary); text-align:left;"
                                    onmouseover="this.style.background='var(--color-bg-hover)'"
                                    onmouseout="this.style.background='none'"
                                    on:click=move |_| {
                                        set_show_add_menu.set(false);
                                        set_show_modal.set(true);
                                    }
                                >
                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <rect x="2" y="3" width="20" height="14" rx="2"/>
                                        <line x1="8" y1="21" x2="16" y2="21"/>
                                        <line x1="12" y1="17" x2="12" y2="21"/>
                                    </svg>
                                    "Service"
                                </button>
                                <button
                                    style="width:100%; background:none; border:none; cursor:pointer; \
                                           display:flex; align-items:center; gap:0.6rem; \
                                           padding:0.5rem 0.75rem; border-radius:0.4rem; \
                                           font-size:0.82rem; color:var(--color-text-primary); text-align:left;"
                                    onmouseover="this.style.background='var(--color-bg-hover)'"
                                    onmouseout="this.style.background='none'"
                                    on:click=move |_| {
                                        set_show_add_menu.set(false);
                                        set_show_add_link_modal.set(true);
                                    }
                                >
                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
                                        <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
                                    </svg>
                                    "Quick Link"
                                </button>
                            </div>
                        </Show>
                    </div>
                </div>
            </div>

            <Suspense fallback=move || view! {
                <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(320px,360px)); gap:1rem; justify-content:start;">
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
                    let mut svcs = svcs;
                    match sort_mode.get() {
                        SortMode::Alphabetical => svcs.sort_by(|a, b| {
                            a.display_name.to_ascii_lowercase().cmp(&b.display_name.to_ascii_lowercase())
                        }),
                        SortMode::Source => svcs.sort_by(|a, b| {
                            let src = |s: &ServiceResponse| {
                                s.discovery_source.clone()
                                    .or_else(|| s.systemd_unit.as_ref().filter(|u| u.ends_with(".service")).map(|_| "systemd".to_string()))
                                    .unwrap_or_default()
                                    .to_ascii_lowercase()
                            };
                            src(a).cmp(&src(b)).then(a.display_name.to_ascii_lowercase().cmp(&b.display_name.to_ascii_lowercase()))
                        }),
                        SortMode::Default => {}
                    }
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
                                        "Use \"+ Add\" above to get started."
                                    </p>
                                </div>
                            </div>
                        })
                    } else {
                        Either::Right(view! {
                            <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(320px,360px)); gap:1rem; justify-content:start;">
                                {svcs.into_iter().map(|svc| {
                                    let id = svc.id;
                                    let edit_form = EditFormData {
                                        display_name: svc.display_name.clone(),
                                        description: svc.description.clone().unwrap_or_default(),
                                        url: svc.url.clone().unwrap_or_default(),
                                        icon: svc.icon.clone().unwrap_or_default(),
                                        group_id: None,
                                        probe_enabled: svc.probe_enabled,
                                        probe_interval: svc.probe_interval,
                                    };
                                    let data = ServiceData {
                                        id: svc.id,
                                        systemd_unit: svc.systemd_unit,
                                        discovery_source: svc.discovery_source,
                                        display_name: svc.display_name,
                                        description: svc.description,
                                        url: svc.url,
                                        icon: svc.icon,
                                        status: svc.status,
                                        latency_ms: svc.latency_ms,
                                    };
                                    let on_delete = Callback::new(move |_: i64| {
                                        spawn_local(async move {
                                            let _ = gloo_net::http::Request::delete(
                                                &format!("/api/v1/services/{id}")
                                            ).send().await;
                                            services.refetch();
                                        });
                                    });
                                    let on_edit = Callback::new(move |_: i64| {
                                        edit_target.set(Some((id, edit_form.clone())));
                                    });
                                    view! { <ServiceCard service=data on_delete=on_delete on_edit=on_edit /> }
                                }).collect_view()}
                            </div>
                        })
                    }
                })}
            </Suspense>

            // ── Quick Links section ───────────────────────────────────────────────
            <Suspense fallback=|| ()>
                {move || quick_links.get().map(|links| {
                    if links.is_empty() {
                        Either::Left(())
                    } else {
                        Either::Right(view! {
                            <div style="margin-top:2rem;">
                                <h2 style="font-size:0.8rem; font-weight:600; text-transform:uppercase; \
                                            letter-spacing:0.08em; color:var(--color-text-muted); margin:0 0 0.75rem;">
                                    "Quick Links"
                                </h2>
                                <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:0.75rem; max-width:1200px;">
                                    {links.into_iter().map(|link| {
                                        let id = link.id;
                                        let edit_form = QuickLinkFormData {
                                            title: link.title.clone(),
                                            url: link.url.clone(),
                                            icon: link.icon.clone().unwrap_or_default(),
                                            description: link.description.clone().unwrap_or_default(),
                                        };
                                        let data = QuickLinkData {
                                            id: link.id,
                                            title: link.title,
                                            url: link.url,
                                            icon: link.icon,
                                            description: link.description,
                                        };
                                        let on_delete = Callback::new(move |_: i64| {
                                            spawn_local(async move {
                                                let _ = gloo_net::http::Request::delete(
                                                    &format!("/api/v1/quick-links/{id}")
                                                ).send().await;
                                                quick_links.refetch();
                                            });
                                        });
                                        let on_edit = Callback::new(move |_: i64| {
                                            edit_link_target.set(Some((id, edit_form.clone())));
                                        });
                                        view! { <QuickLinkCard link=data on_delete=on_delete on_edit=on_edit /> }
                                    }).collect_view()}
                                </div>
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
    Ok(resp.json().await?)
}

async fn fetch_quick_links() -> Result<Vec<QuickLinkResponse>, gloo_net::Error> {
    let resp = gloo_net::http::Request::get("/api/v1/quick-links")
        .send()
        .await?;
    Ok(resp.json().await?)
}
