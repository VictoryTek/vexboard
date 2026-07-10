use std::collections::HashMap;

use leptos::either::EitherOf4;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::modal_edit::EditFormData;
use crate::components::service_card::{ServiceCard, ServiceData};
use crate::CurrentUser;

use super::{fetch_services, reorder_services, GroupResponse, ServiceResponse, SortMode};

/// Wire shape of a `probe` event from `/api/v1/services/stream`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, serde::Deserialize)]
struct ProbeEventFe {
    service_id: i64,
    status: String,
    latency_ms: Option<i64>,
}

#[component]
pub(super) fn ServiceGrid(
    services: LocalResource<Vec<ServiceResponse>>,
    groups: LocalResource<Vec<GroupResponse>>,
    sort_mode: ReadSignal<SortMode>,
    drag_src_idx: RwSignal<Option<usize>>,
    drag_over_idx: RwSignal<Option<usize>>,
    section_drag_src: RwSignal<Option<(String, usize)>>,
    section_drag_over: RwSignal<Option<(String, usize)>>,
    edit_target: RwSignal<Option<(i64, EditFormData)>>,
) -> impl IntoView {
    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    // Live status/latency overrides patched in from the probe SSE stream, keyed
    // by service id. Merged over the last fetched snapshot at render time so
    // cards reflect probe results without waiting for a full refetch.
    let live_status = RwSignal::new(HashMap::<i64, (String, Option<i64>)>::new());

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        use web_sys::EventSource;

        Effect::new(move |_| {
            let es = EventSource::new("/api/v1/services/stream").ok();
            if let Some(es) = es {
                let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                    if let Some(data) = event.data().as_string() {
                        if let Ok(probe) = serde_json::from_str::<ProbeEventFe>(&data) {
                            live_status.update(|m| {
                                m.insert(probe.service_id, (probe.status, probe.latency_ms));
                            });
                        }
                    }
                }) as Box<dyn FnMut(_)>);

                es.add_event_listener_with_callback("probe", on_message.as_ref().unchecked_ref())
                    .ok();
                on_message.forget();
            }
        });
    }

    view! {
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
            {move || {
                services.get().map(|svcs| {
                let render_card = move |svc: ServiceResponse| {
                    let id = svc.id;
                    let edit_form = EditFormData {
                        display_name: svc.display_name.clone(),
                        description: svc.description.clone().unwrap_or_default(),
                        url: svc.url.clone().unwrap_or_default(),
                        icon: svc.icon.clone().unwrap_or_default(),
                        group_id: svc.group_id,
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
                        probe_enabled: svc.probe_enabled,
                    };
                    let (on_delete, on_edit) = if is_admin() {
                        let cb_delete = Callback::new(move |_: i64| {
                            spawn_local(async move {
                                let _ = gloo_net::http::Request::delete(
                                    &format!("/api/v1/services/{id}")
                                ).send().await;
                                services.refetch();
                            });
                        });
                        let cb_edit = Callback::new(move |_: i64| {
                            edit_target.set(Some((id, edit_form.clone())));
                        });
                        (Some(cb_delete), Some(cb_edit))
                    } else {
                        (None, None)
                    };
                    view! { <ServiceCard service=data live_status=live_status on_delete=on_delete on_edit=on_edit /> }
                };

                if svcs.is_empty() {
                    EitherOf4::A(view! {
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
                } else if sort_mode.get() == SortMode::Group {
                    let group_list = groups.get().unwrap_or_default();
                    let known_ids: std::collections::HashSet<i64> =
                        group_list.iter().map(|g| g.id).collect();

                    type Section = (String, String, String, String, String, Vec<ServiceResponse>);
                    let mut sections_data: Vec<Section> = group_list.iter().filter_map(|grp| {
                        let gid = grp.id;
                        let mut members: Vec<ServiceResponse> = svcs.iter()
                            .filter(|s| s.group_id == Some(gid))
                            .cloned()
                            .collect();
                        if members.is_empty() { return None; }
                        members.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                        let (text_color, bg_color, border_color) = match &grp.color {
                            Some(hex) => (hex.clone(), format!("{hex}22"), format!("{hex}50")),
                            None => (
                                "var(--color-accent)".to_string(),
                                "var(--color-accent-dim)".to_string(),
                                "rgba(59,130,246,0.3)".to_string(),
                            ),
                        };
                        Some((gid.to_string(), grp.name.clone(), text_color, bg_color, border_color, members))
                    }).collect();

                    let mut ungrouped: Vec<ServiceResponse> = svcs.iter()
                        .filter(|s| s.group_id.is_none_or(|gid| !known_ids.contains(&gid)))
                        .cloned()
                        .collect();
                    if !ungrouped.is_empty() {
                        ungrouped.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                        sections_data.push((
                            "ungrouped".to_string(),
                            "Ungrouped".to_string(),
                            "var(--color-text-muted)".to_string(),
                            "rgba(75,85,99,0.12)".to_string(),
                            "rgba(75,85,99,0.2)".to_string(),
                            ungrouped,
                        ));
                    }

                    let sections = sections_data.into_iter().map(|(sec_key, label, color, bg, border, members)| {
                        let member_ids: Vec<i64> = members.iter().map(|s| s.id).collect();
                        let reset_payload: Vec<(i64, i64)> = {
                            let mut sorted = members.clone();
                            sorted.sort_by_key(|a| a.display_name.to_lowercase());
                            sorted.iter().enumerate().map(|(i, s)| (s.id, i as i64)).collect()
                        };
                        let members_with_idx: Vec<(usize, ServiceResponse)> =
                            members.into_iter().enumerate().collect();
                        let cards = members_with_idx.into_iter().map(|(idx, svc)| {
                            let card = render_card(svc);
                            let ids_for_drop = member_ids.clone();
                            let sk_style = sec_key.clone();
                            let sk_start = sec_key.clone();
                            let sk_over  = sec_key.clone();
                            let sk_leave = sec_key.clone();
                            view! {
                                <div
                                    draggable="true"
                                    style={
                                        let sk = sk_style;
                                        move || {
                                            let is_over     = section_drag_over.get() == Some((sk.clone(), idx));
                                            let is_dragging = section_drag_src.get()  == Some((sk.clone(), idx));
                                            let mut s = "cursor:grab;".to_string();
                                            if is_dragging { s.push_str("opacity:0.45;"); }
                                            if is_over     { s.push_str("outline:2px solid var(--color-accent);border-radius:12px;"); }
                                            s
                                        }
                                    }
                                    on:dragstart=move |_| section_drag_src.set(Some((sk_start.clone(), idx)))
                                    on:dragover=move |ev| {
                                        ev.prevent_default();
                                        section_drag_over.set(Some((sk_over.clone(), idx)));
                                    }
                                    on:dragleave=move |_| {
                                        if section_drag_over.get() == Some((sk_leave.clone(), idx)) {
                                            section_drag_over.set(None);
                                        }
                                    }
                                    on:drop=move |ev| {
                                        ev.prevent_default();
                                        let src = section_drag_src.get();
                                        let dst = section_drag_over.get();
                                        section_drag_src.set(None);
                                        section_drag_over.set(None);
                                        if let (Some((src_sec, src_i)), Some((dst_sec, dst_i))) = (src, dst) {
                                            if src_sec == dst_sec && src_i != dst_i {
                                                let ids: std::collections::HashSet<i64> =
                                                    ids_for_drop.iter().cloned().collect();
                                                spawn_local(async move {
                                                    if let Ok(all) = fetch_services().await {
                                                        let mut section: Vec<ServiceResponse> = all.into_iter()
                                                            .filter(|s| ids.contains(&s.id))
                                                            .collect();
                                                        section.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                                                            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                                                        let item = section.remove(src_i);
                                                        section.insert(dst_i, item);
                                                        let payload: Vec<_> = section.iter()
                                                            .enumerate()
                                                            .map(|(i, s)| (s.id, i as i64))
                                                            .collect();
                                                        let _ = reorder_services(payload).await;
                                                        services.refetch();
                                                    }
                                                });
                                            }
                                        }
                                    }
                                    on:dragend=move |_| {
                                        section_drag_src.set(None);
                                        section_drag_over.set(None);
                                    }
                                >
                                    {card}
                                </div>
                            }
                        }).collect_view();
                        view! {
                            <div style="margin-bottom:1.75rem;">
                                <div style="display:flex; align-items:center; gap:0.6rem; margin-bottom:0.75rem;">
                                    <span style=format!(
                                        "display:inline-flex; align-items:center; font-size:0.68rem; font-weight:700; \
                                         letter-spacing:0.04em; text-transform:uppercase; \
                                         color:{color}; background:{bg}; border:1px solid {border}; \
                                         border-radius:20px; padding:3px 9px;"
                                    )>{label}</span>
                                    <div style="flex:1; height:1px; background:var(--color-border); opacity:0.4;"></div>
                                    <button
                                        title="Reset section to A-Z"
                                        style="display:inline-flex; align-items:center; justify-content:center; \
                                               background:none; border:none; cursor:pointer; padding:3px; \
                                               color:var(--color-text-muted); border-radius:4px; transition:color 0.15s;"
                                        onmouseover="this.style.color='var(--color-text-primary)'"
                                        onmouseout="this.style.color='var(--color-text-muted)'"
                                        on:click=move |_| {
                                            let payload = reset_payload.clone();
                                            spawn_local(async move {
                                                let _ = reorder_services(payload).await;
                                                services.refetch();
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
                                </div>
                                <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(320px,360px)); gap:1rem; justify-content:start;">
                                    {cards}
                                </div>
                            </div>
                        }
                    }).collect_view();
                    EitherOf4::B(view! { <div>{sections}</div> })
                } else if sort_mode.get() == SortMode::Source {
                    let get_src = |s: &ServiceResponse| -> String {
                        s.discovery_source.clone()
                            .or_else(|| s.systemd_unit.as_ref().filter(|u| u.ends_with(".service")).map(|_| "systemd".to_string()))
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                    };
                    let source_order: &[(&str, &str, &str)] = &[
                        ("docker",  "Docker",  "#0db7ed"),
                        ("podman",  "Podman",  "#892ca0"),
                        ("systemd", "Systemd", "#e8873a"),
                        ("",        "Manual",  "#6b7280"),
                    ];
                    let sections = source_order.iter().filter_map(|(src_key, label, color)| {
                        let mut group: Vec<ServiceResponse> = svcs.iter()
                            .filter(|s| get_src(s) == *src_key)
                            .cloned()
                            .collect();
                        if group.is_empty() { return None; }
                        group.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                        let color = color.to_string();
                        let label = label.to_string();
                        let sec_key = src_key.to_string();
                        let member_ids: Vec<i64> = group.iter().map(|s| s.id).collect();
                        let reset_payload: Vec<(i64, i64)> = {
                            let mut sorted = group.clone();
                            sorted.sort_by_key(|a| a.display_name.to_lowercase());
                            sorted.iter().enumerate().map(|(i, s)| (s.id, i as i64)).collect()
                        };
                        let members_with_idx: Vec<(usize, ServiceResponse)> =
                            group.into_iter().enumerate().collect();
                        let cards = members_with_idx.into_iter().map(|(idx, svc)| {
                            let card = render_card(svc);
                            let ids_for_drop = member_ids.clone();
                            let sk_style = sec_key.clone();
                            let sk_start = sec_key.clone();
                            let sk_over  = sec_key.clone();
                            let sk_leave = sec_key.clone();
                            view! {
                                <div
                                    draggable="true"
                                    style={
                                        let sk = sk_style;
                                        move || {
                                            let is_over     = section_drag_over.get() == Some((sk.clone(), idx));
                                            let is_dragging = section_drag_src.get()  == Some((sk.clone(), idx));
                                            let mut s = "cursor:grab;".to_string();
                                            if is_dragging { s.push_str("opacity:0.45;"); }
                                            if is_over     { s.push_str("outline:2px solid var(--color-accent);border-radius:12px;"); }
                                            s
                                        }
                                    }
                                    on:dragstart=move |_| section_drag_src.set(Some((sk_start.clone(), idx)))
                                    on:dragover=move |ev| {
                                        ev.prevent_default();
                                        section_drag_over.set(Some((sk_over.clone(), idx)));
                                    }
                                    on:dragleave=move |_| {
                                        if section_drag_over.get() == Some((sk_leave.clone(), idx)) {
                                            section_drag_over.set(None);
                                        }
                                    }
                                    on:drop=move |ev| {
                                        ev.prevent_default();
                                        let src = section_drag_src.get();
                                        let dst = section_drag_over.get();
                                        section_drag_src.set(None);
                                        section_drag_over.set(None);
                                        if let (Some((src_sec, src_i)), Some((dst_sec, dst_i))) = (src, dst) {
                                            if src_sec == dst_sec && src_i != dst_i {
                                                let ids: std::collections::HashSet<i64> =
                                                    ids_for_drop.iter().cloned().collect();
                                                spawn_local(async move {
                                                    if let Ok(all) = fetch_services().await {
                                                        let mut section: Vec<ServiceResponse> = all.into_iter()
                                                            .filter(|s| ids.contains(&s.id))
                                                            .collect();
                                                        section.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                                                            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                                                        let item = section.remove(src_i);
                                                        section.insert(dst_i, item);
                                                        let payload: Vec<_> = section.iter()
                                                            .enumerate()
                                                            .map(|(i, s)| (s.id, i as i64))
                                                            .collect();
                                                        let _ = reorder_services(payload).await;
                                                        services.refetch();
                                                    }
                                                });
                                            }
                                        }
                                    }
                                    on:dragend=move |_| {
                                        section_drag_src.set(None);
                                        section_drag_over.set(None);
                                    }
                                >
                                    {card}
                                </div>
                            }
                        }).collect_view();
                        Some(view! {
                            <div style="margin-bottom:1.75rem;">
                                <div style="display:flex; align-items:center; gap:0.6rem; margin-bottom:0.75rem;">
                                    <span style=format!(
                                        "display:inline-flex; align-items:center; font-size:0.68rem; font-weight:700; \
                                         letter-spacing:0.04em; text-transform:uppercase; \
                                         color:{color}; background:{color}22; border:1px solid {color}40; \
                                         border-radius:20px; padding:3px 9px;"
                                    )>{label}</span>
                                    <div style="flex:1; height:1px; background:var(--color-border); opacity:0.4;"></div>
                                    <button
                                        title="Reset section to A-Z"
                                        style="display:inline-flex; align-items:center; justify-content:center; \
                                               background:none; border:none; cursor:pointer; padding:3px; \
                                               color:var(--color-text-muted); border-radius:4px; transition:color 0.15s;"
                                        onmouseover="this.style.color='var(--color-text-primary)'"
                                        onmouseout="this.style.color='var(--color-text-muted)'"
                                        on:click=move |_| {
                                            let payload = reset_payload.clone();
                                            spawn_local(async move {
                                                let _ = reorder_services(payload).await;
                                                services.refetch();
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
                                </div>
                                <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(320px,360px)); gap:1rem; justify-content:start;">
                                    {cards}
                                </div>
                            </div>
                        })
                    }).collect_view();
                    EitherOf4::C(view! { <div>{sections}</div> })
                } else {
                    let mut svcs = svcs;
                    svcs.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                        .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                    let svcs_with_idx: Vec<(usize, ServiceResponse)> =
                        svcs.into_iter().enumerate().collect();
                    let cards = svcs_with_idx.into_iter().map(|(idx, svc)| {
                        let card = render_card(svc);
                        view! {
                            <div
                                draggable="true"
                                style=move || {
                                    let is_over = drag_over_idx.get() == Some(idx);
                                    let is_dragging = drag_src_idx.get() == Some(idx);
                                    let mut s = "cursor:grab;".to_string();
                                    if is_dragging { s.push_str("opacity:0.45;"); }
                                    if is_over { s.push_str("outline:2px solid var(--color-accent);border-radius:12px;"); }
                                    s
                                }
                                on:dragstart=move |_| drag_src_idx.set(Some(idx))
                                on:dragover=move |ev| {
                                    ev.prevent_default();
                                    drag_over_idx.set(Some(idx));
                                }
                                on:dragleave=move |_| {
                                    if drag_over_idx.get() == Some(idx) {
                                        drag_over_idx.set(None);
                                    }
                                }
                                on:drop=move |ev| {
                                    ev.prevent_default();
                                    let src = drag_src_idx.get();
                                    let dst = drag_over_idx.get();
                                    drag_src_idx.set(None);
                                    drag_over_idx.set(None);
                                    if let (Some(src_i), Some(dst_i)) = (src, dst) {
                                        if src_i != dst_i {
                                            spawn_local(async move {
                                                if let Ok(mut current) = fetch_services().await {
                                                    let item = current.remove(src_i);
                                                    current.insert(dst_i, item);
                                                    let payload: Vec<_> = current.iter()
                                                        .enumerate()
                                                        .map(|(i, s)| (s.id, i as i64))
                                                        .collect();
                                                    let _ = reorder_services(payload).await;
                                                    services.refetch();
                                                }
                                            });
                                        }
                                    }
                                }
                                on:dragend=move |_| {
                                    drag_src_idx.set(None);
                                    drag_over_idx.set(None);
                                }
                            >
                                {card}
                            </div>
                        }
                    }).collect_view();
                    EitherOf4::D(view! {
                        <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(320px,360px)); gap:1rem; justify-content:start;">
                            {cards}
                        </div>
                    })
                }
            })
            }}
        </Suspense>
    }
}
