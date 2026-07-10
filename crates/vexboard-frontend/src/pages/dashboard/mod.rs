mod modals;
mod quick_links_section;
mod service_grid;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::modal_edit::{EditFormData, GroupItem};
use crate::components::quick_link_modal::QuickLinkFormData;
use crate::CurrentUser;

use modals::DashboardModals;
use quick_links_section::QuickLinksSection;
use service_grid::ServiceGrid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum SortMode {
    AZ,
    Source,
    Group,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct ServiceResponse {
    pub id: i64,
    pub systemd_unit: Option<String>,
    pub discovery_source: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: i64,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub probe_enabled: bool,
    pub probe_interval: i64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct GroupResponse {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct QuickLinkResponse {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct QuickLinkGroupResponse {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

pub(super) fn resolve_groups(groups: &LocalResource<Vec<GroupResponse>>) -> Vec<GroupItem> {
    groups
        .get()
        .map(|g| {
            g.iter()
                .map(|r| GroupItem {
                    id: r.id,
                    name: r.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn resolve_quick_link_groups(
    groups: &LocalResource<Vec<QuickLinkGroupResponse>>,
) -> Vec<GroupItem> {
    groups
        .get()
        .map(|g| {
            g.iter()
                .map(|r| GroupItem {
                    id: r.id,
                    name: r.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[component]
pub fn DashboardPage() -> impl IntoView {
    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    let services = LocalResource::new(|| async move { fetch_services().await.unwrap_or_default() });
    let quick_links =
        LocalResource::new(|| async move { fetch_quick_links().await.unwrap_or_default() });
    let groups = LocalResource::new(|| async move { fetch_groups().await.unwrap_or_default() });
    let quick_link_groups =
        LocalResource::new(|| async move { fetch_quick_link_groups().await.unwrap_or_default() });

    let show_modal: RwSignal<bool> = RwSignal::new(false);
    let show_add_link_modal: RwSignal<bool> = RwSignal::new(false);
    let show_groups_modal: RwSignal<bool> = RwSignal::new(false);
    let show_quick_link_groups_modal: RwSignal<bool> = RwSignal::new(false);
    let (show_add_menu, set_show_add_menu) = signal(false);
    let (sort_mode, set_sort_mode) = signal(SortMode::AZ);
    let (ql_sort_mode, set_ql_sort_mode) = signal(SortMode::AZ);

    let drag_src_idx: RwSignal<Option<usize>> = RwSignal::new(None);
    let drag_over_idx: RwSignal<Option<usize>> = RwSignal::new(None);
    let section_drag_src: RwSignal<Option<(String, usize)>> = RwSignal::new(None);
    let section_drag_over: RwSignal<Option<(String, usize)>> = RwSignal::new(None);

    let ql_drag_src_idx: RwSignal<Option<usize>> = RwSignal::new(None);
    let ql_drag_over_idx: RwSignal<Option<usize>> = RwSignal::new(None);
    let ql_section_drag_src: RwSignal<Option<(String, usize)>> = RwSignal::new(None);
    let ql_section_drag_over: RwSignal<Option<(String, usize)>> = RwSignal::new(None);

    let edit_target: RwSignal<Option<(i64, EditFormData)>> = RwSignal::new(None);
    let edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>> = RwSignal::new(None);

    view! {
        <DashboardModals
            services=services
            quick_links=quick_links
            groups=groups
            quick_link_groups=quick_link_groups
            show_modal=show_modal
            show_add_link_modal=show_add_link_modal
            show_groups_modal=show_groups_modal
            show_quick_link_groups_modal=show_quick_link_groups_modal
            edit_target=edit_target
            edit_link_target=edit_link_target
        />

        <div>
            // ── Page header: sort controls + "Add" dropdown ──────────────────────
            <div class="page-header">
                <h1 class="page-title">"Services"</h1>
                <div style="display:flex; align-items:center; gap:0.5rem;">
                    // Sort controls
                    <div style="display:flex; align-items:center; gap:0.25rem; \
                                background:var(--color-bg-surface); border:1px solid var(--color-border); \
                                border-radius:0.5rem; padding:0.2rem;">
                        {[
                            (SortMode::AZ,     "A-Z"),
                            (SortMode::Source, "Source"),
                            (SortMode::Group,  "Group"),
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

                    // Reset order (A-Z mode only)
                    {move || (sort_mode.get() == SortMode::AZ).then(|| view! {
                        <button
                            title="Reset order to A-Z"
                            style="display:inline-flex; align-items:center; justify-content:center; \
                                   background:var(--color-bg-surface); border:1px solid var(--color-border); \
                                   cursor:pointer; padding:0.22rem 0.45rem; color:var(--color-text-muted); \
                                   border-radius:0.5rem; transition:color 0.15s;"
                            onmouseover="this.style.color='var(--color-text-primary)'"
                            onmouseout="this.style.color='var(--color-text-muted)'"
                            on:click=move |_| {
                                spawn_local(async move {
                                    if let Ok(mut all) = fetch_services().await {
                                        all.sort_by_key(|a| a.display_name.to_lowercase());
                                        let payload: Vec<_> = all.iter()
                                            .enumerate()
                                            .map(|(i, s)| (s.id, i as i64))
                                            .collect();
                                        let _ = reorder_services(payload).await;
                                        services.refetch();
                                    }
                                });
                            }
                        >
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.2"
                                 stroke-linecap="round" stroke-linejoin="round">
                                <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>
                                <path d="M3 3v5h5"/>
                            </svg>
                        </button>
                    })}

                    // "+ Add" dropdown — admin only
                    <Show when=move || is_admin()>
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
                                        show_modal.set(true);
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
                                        show_add_link_modal.set(true);
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
                                <div style="height:1px; background:var(--color-border); margin:0.2rem 0.4rem;"></div>
                                <button
                                    style="width:100%; background:none; border:none; cursor:pointer; \
                                           display:flex; align-items:center; gap:0.6rem; \
                                           padding:0.5rem 0.75rem; border-radius:0.4rem; \
                                           font-size:0.82rem; color:var(--color-text-primary); text-align:left;"
                                    onmouseover="this.style.background='var(--color-bg-hover)'"
                                    onmouseout="this.style.background='none'"
                                    on:click=move |_| {
                                        set_show_add_menu.set(false);
                                        show_groups_modal.set(true);
                                    }
                                >
                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                                    </svg>
                                    "Manage Groups"
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
                                        show_quick_link_groups_modal.set(true);
                                    }
                                >
                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                                    </svg>
                                    "Manage Quick Link Groups"
                                </button>
                            </div>
                        </Show>
                    </div>
                    </Show>
                </div>
            </div>

            // ── Service grid ──────────────────────────────────────────────────────
            <ServiceGrid
                services=services
                groups=groups
                sort_mode=sort_mode
                drag_src_idx=drag_src_idx
                drag_over_idx=drag_over_idx
                section_drag_src=section_drag_src
                section_drag_over=section_drag_over
                edit_target=edit_target
            />

            // ── Quick links ───────────────────────────────────────────────────────
            <QuickLinksSection
                quick_links=quick_links
                groups=quick_link_groups
                sort_mode=ql_sort_mode
                set_sort_mode=set_ql_sort_mode
                drag_src_idx=ql_drag_src_idx
                drag_over_idx=ql_drag_over_idx
                section_drag_src=ql_section_drag_src
                section_drag_over=ql_section_drag_over
                edit_link_target=edit_link_target
            />
        </div>
    }
}

pub(super) async fn fetch_services() -> Result<Vec<ServiceResponse>, gloo_net::Error> {
    let resp = gloo_net::http::Request::get("/api/v1/services")
        .send()
        .await?;
    resp.json().await
}

pub(super) async fn fetch_groups() -> Result<Vec<GroupResponse>, gloo_net::Error> {
    let resp = gloo_net::http::Request::get("/api/v1/groups")
        .send()
        .await?;
    resp.json().await
}

pub(super) async fn fetch_quick_links() -> Result<Vec<QuickLinkResponse>, gloo_net::Error> {
    let resp = gloo_net::http::Request::get("/api/v1/quick-links")
        .send()
        .await?;
    resp.json().await
}

pub(super) async fn fetch_quick_link_groups() -> Result<Vec<QuickLinkGroupResponse>, gloo_net::Error>
{
    let resp = gloo_net::http::Request::get("/api/v1/quick-link-groups")
        .send()
        .await?;
    resp.json().await
}

pub(super) async fn reorder_services(items: Vec<(i64, i64)>) -> Result<(), gloo_net::Error> {
    let body: Vec<_> = items
        .iter()
        .map(|(id, so)| serde_json::json!({"id": id, "sort_order": so}))
        .collect();
    gloo_net::http::Request::patch("/api/v1/services/reorder")
        .json(&body)?
        .send()
        .await?;
    Ok(())
}

pub(super) async fn reorder_quick_links(items: Vec<(i64, i64)>) -> Result<(), gloo_net::Error> {
    let body: Vec<_> = items
        .iter()
        .map(|(id, so)| serde_json::json!({"id": id, "sort_order": so}))
        .collect();
    gloo_net::http::Request::patch("/api/v1/quick-links/reorder")
        .json(&body)?
        .send()
        .await?;
    Ok(())
}
