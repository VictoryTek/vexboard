use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::modal_edit::EditFormData;
use crate::components::quick_link_card::{QuickLinkCard, QuickLinkData};
use crate::components::quick_link_modal::QuickLinkFormData;
use crate::components::service_card::{ServiceCard, ServiceData};
use crate::CurrentUser;

use super::{
    fetch_quick_links, fetch_services, reorder_quick_links, reorder_services, GroupResponse,
    QuickLinkResponse, ServiceResponse,
};

/// Renders every group (plus an "Ungrouped" bucket) as one container per group,
/// with that group's services in a row above its quick links — used only in
/// `SortMode::Group`, since a group's services and quick links are otherwise
/// rendered by the separate `ServiceGrid`/`QuickLinksSection` components.
#[component]
pub(super) fn GroupSection(
    services: LocalResource<Vec<ServiceResponse>>,
    quick_links: LocalResource<Vec<QuickLinkResponse>>,
    groups: LocalResource<Vec<GroupResponse>>,
    live_status: RwSignal<HashMap<i64, (String, Option<i64>)>>,
    svc_section_drag_src: RwSignal<Option<(String, usize)>>,
    svc_section_drag_over: RwSignal<Option<(String, usize)>>,
    ql_section_drag_src: RwSignal<Option<(String, usize)>>,
    ql_section_drag_over: RwSignal<Option<(String, usize)>>,
    edit_target: RwSignal<Option<(i64, EditFormData)>>,
    edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>>,
) -> impl IntoView {
    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    let render_service_card = move |svc: ServiceResponse| {
        let id = svc.id;
        let edit_form = EditFormData {
            display_name: svc.display_name.clone(),
            description: svc.description.clone().unwrap_or_default(),
            url: svc.url.clone().unwrap_or_default(),
            icon: svc.icon.clone().unwrap_or_default(),
            group_id: svc.group_id,
            probe_enabled: svc.probe_enabled,
            probe_interval: svc.probe_interval,
            skip_tls_verify: svc.skip_tls_verify,
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
                    let _ = gloo_net::http::Request::delete(&format!("/api/v1/services/{id}"))
                        .send()
                        .await;
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

    let render_link_card = move |link: QuickLinkResponse| {
        let id = link.id;
        let edit_form = QuickLinkFormData {
            title: link.title.clone(),
            url: link.url.clone(),
            icon: link.icon.clone().unwrap_or_default(),
            description: link.description.clone().unwrap_or_default(),
            group_id: link.group_id,
        };
        let data = QuickLinkData {
            id: link.id,
            title: link.title,
            url: link.url,
            icon: link.icon,
            description: link.description,
        };
        let (on_delete, on_edit) = if is_admin() {
            let cb_delete = Callback::new(move |_: i64| {
                spawn_local(async move {
                    let _ = gloo_net::http::Request::delete(&format!("/api/v1/quick-links/{id}"))
                        .send()
                        .await;
                    quick_links.refetch();
                });
            });
            let cb_edit = Callback::new(move |_: i64| {
                edit_link_target.set(Some((id, edit_form.clone())));
            });
            (Some(cb_delete), Some(cb_edit))
        } else {
            (None, None)
        };
        view! { <QuickLinkCard link=data on_delete=on_delete on_edit=on_edit /> }
    };

    view! {
        // See ServiceGrid: Suspense re-shows its fallback on every post-load resource
        // change, so a probe tick blanked this whole section (fallback is `()`).
        <Transition fallback=|| ()>
            {move || {
                let svcs = services.get().unwrap_or_default();
                let links = quick_links.get().unwrap_or_default();
                let group_list = groups.get().unwrap_or_default();

                if svcs.is_empty() && links.is_empty() {
                    return ().into_any();
                }

                let known_ids: std::collections::HashSet<i64> =
                    group_list.iter().map(|g| g.id).collect();

                type Section = (String, String, String, String, String, Vec<ServiceResponse>, Vec<QuickLinkResponse>);
                let mut sections_data: Vec<Section> = group_list.iter().filter_map(|grp| {
                    let gid = grp.id;
                    let mut svc_members: Vec<ServiceResponse> = svcs.iter()
                        .filter(|s| s.group_id == Some(gid))
                        .cloned()
                        .collect();
                    let mut link_members: Vec<QuickLinkResponse> = links.iter()
                        .filter(|l| l.group_id == Some(gid))
                        .cloned()
                        .collect();
                    if svc_members.is_empty() && link_members.is_empty() { return None; }
                    svc_members.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                        .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                    link_members.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
                    let (text_color, bg_color, border_color) = match &grp.color {
                        Some(hex) => (hex.clone(), format!("{hex}22"), format!("{hex}50")),
                        None => (
                            "var(--color-accent)".to_string(),
                            "var(--color-accent-dim)".to_string(),
                            "rgba(59,130,246,0.3)".to_string(),
                        ),
                    };
                    Some((gid.to_string(), grp.name.clone(), text_color, bg_color, border_color, svc_members, link_members))
                }).collect();
                sections_data.sort_by_key(|s| s.1.to_lowercase());

                let mut ungrouped_svcs: Vec<ServiceResponse> = svcs.iter()
                    .filter(|s| s.group_id.is_none_or(|gid| !known_ids.contains(&gid)))
                    .cloned()
                    .collect();
                let mut ungrouped_links: Vec<QuickLinkResponse> = links.iter()
                    .filter(|l| l.group_id.is_none_or(|gid| !known_ids.contains(&gid)))
                    .cloned()
                    .collect();
                if !ungrouped_svcs.is_empty() || !ungrouped_links.is_empty() {
                    ungrouped_svcs.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                        .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
                    ungrouped_links.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
                    sections_data.push((
                        "ungrouped".to_string(),
                        "Ungrouped".to_string(),
                        "var(--color-text-muted)".to_string(),
                        "rgba(75,85,99,0.12)".to_string(),
                        "rgba(75,85,99,0.2)".to_string(),
                        ungrouped_svcs,
                        ungrouped_links,
                    ));
                }

                let sections = sections_data.into_iter().map(|(sec_key, label, color, bg, border, svc_members, link_members)| {
                    // Services row
                    let svc_member_ids: Vec<i64> = svc_members.iter().map(|s| s.id).collect();
                    let svc_reset_payload: Vec<(i64, i64)> = {
                        let mut sorted = svc_members.clone();
                        sorted.sort_by_key(|a| a.display_name.to_lowercase());
                        sorted.iter().enumerate().map(|(i, s)| (s.id, i as i64)).collect()
                    };
                    let svc_sec_key = sec_key.clone();
                    let svc_cards = svc_members.into_iter().enumerate().map(|(idx, svc)| {
                        let card = render_service_card(svc);
                        let ids_for_drop = svc_member_ids.clone();
                        let sk_style = svc_sec_key.clone();
                        let sk_start = svc_sec_key.clone();
                        let sk_over  = svc_sec_key.clone();
                        let sk_leave = svc_sec_key.clone();
                        view! {
                            <div
                                draggable="true"
                                style={
                                    let sk = sk_style;
                                    move || {
                                        let is_over     = svc_section_drag_over.get() == Some((sk.clone(), idx));
                                        let is_dragging = svc_section_drag_src.get()  == Some((sk.clone(), idx));
                                        let mut s = "cursor:grab;".to_string();
                                        if is_dragging { s.push_str("opacity:0.45;"); }
                                        if is_over     { s.push_str("outline:2px solid var(--color-accent);border-radius:12px;"); }
                                        s
                                    }
                                }
                                on:dragstart=move |_| svc_section_drag_src.set(Some((sk_start.clone(), idx)))
                                on:dragover=move |ev| {
                                    ev.prevent_default();
                                    svc_section_drag_over.set(Some((sk_over.clone(), idx)));
                                }
                                on:dragleave=move |_| {
                                    if svc_section_drag_over.get() == Some((sk_leave.clone(), idx)) {
                                        svc_section_drag_over.set(None);
                                    }
                                }
                                on:drop=move |ev| {
                                    ev.prevent_default();
                                    let src = svc_section_drag_src.get();
                                    let dst = svc_section_drag_over.get();
                                    svc_section_drag_src.set(None);
                                    svc_section_drag_over.set(None);
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
                                    svc_section_drag_src.set(None);
                                    svc_section_drag_over.set(None);
                                }
                            >
                                {card}
                            </div>
                        }
                    }).collect_view();

                    let svc_row = (!svc_reset_payload.is_empty()).then(|| view! {
                        <div class="grid-cards-320" style="gap:1rem; justify-content:start; margin-bottom:0.75rem;">
                            {svc_cards}
                        </div>
                    });

                    // Quick links row
                    let link_member_ids: Vec<i64> = link_members.iter().map(|l| l.id).collect();
                    let link_reset_payload: Vec<(i64, i64)> = {
                        let mut sorted = link_members.clone();
                        sorted.sort_by_key(|a| a.title.to_lowercase());
                        sorted.iter().enumerate().map(|(i, l)| (l.id, i as i64)).collect()
                    };
                    let ql_sec_key = sec_key.clone();
                    let link_cards = link_members.into_iter().enumerate().map(|(idx, link)| {
                        let card = render_link_card(link);
                        let ids_for_drop = link_member_ids.clone();
                        let sk_style = ql_sec_key.clone();
                        let sk_start = ql_sec_key.clone();
                        let sk_over  = ql_sec_key.clone();
                        let sk_leave = ql_sec_key.clone();
                        view! {
                            <div
                                draggable="true"
                                style={
                                    let sk = sk_style;
                                    move || {
                                        let is_over     = ql_section_drag_over.get() == Some((sk.clone(), idx));
                                        let is_dragging = ql_section_drag_src.get()  == Some((sk.clone(), idx));
                                        let mut s = "cursor:grab;".to_string();
                                        if is_dragging { s.push_str("opacity:0.45;"); }
                                        if is_over     { s.push_str("outline:2px solid var(--color-accent);border-radius:12px;"); }
                                        s
                                    }
                                }
                                on:dragstart=move |_| ql_section_drag_src.set(Some((sk_start.clone(), idx)))
                                on:dragover=move |ev| {
                                    ev.prevent_default();
                                    ql_section_drag_over.set(Some((sk_over.clone(), idx)));
                                }
                                on:dragleave=move |_| {
                                    if ql_section_drag_over.get() == Some((sk_leave.clone(), idx)) {
                                        ql_section_drag_over.set(None);
                                    }
                                }
                                on:drop=move |ev| {
                                    ev.prevent_default();
                                    let src = ql_section_drag_src.get();
                                    let dst = ql_section_drag_over.get();
                                    ql_section_drag_src.set(None);
                                    ql_section_drag_over.set(None);
                                    if let (Some((src_sec, src_i)), Some((dst_sec, dst_i))) = (src, dst) {
                                        if src_sec == dst_sec && src_i != dst_i {
                                            let ids: std::collections::HashSet<i64> =
                                                ids_for_drop.iter().cloned().collect();
                                            spawn_local(async move {
                                                if let Ok(all) = fetch_quick_links().await {
                                                    let mut section: Vec<QuickLinkResponse> = all.into_iter()
                                                        .filter(|l| ids.contains(&l.id))
                                                        .collect();
                                                    section.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                                                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
                                                    let item = section.remove(src_i);
                                                    section.insert(dst_i, item);
                                                    let payload: Vec<_> = section.iter()
                                                        .enumerate()
                                                        .map(|(i, l)| (l.id, i as i64))
                                                        .collect();
                                                    let _ = reorder_quick_links(payload).await;
                                                    quick_links.refetch();
                                                }
                                            });
                                        }
                                    }
                                }
                                on:dragend=move |_| {
                                    ql_section_drag_src.set(None);
                                    ql_section_drag_over.set(None);
                                }
                            >
                                {card}
                            </div>
                        }
                    }).collect_view();

                    let link_row = (!link_reset_payload.is_empty()).then(|| view! {
                        <div class="grid-cards-200" style="gap:0.75rem;">
                            {link_cards}
                        </div>
                    });

                    let reset_both = move |_| {
                        let svc_payload = svc_reset_payload.clone();
                        let link_payload = link_reset_payload.clone();
                        spawn_local(async move {
                            if !svc_payload.is_empty() {
                                let _ = reorder_services(svc_payload).await;
                                services.refetch();
                            }
                            if !link_payload.is_empty() {
                                let _ = reorder_quick_links(link_payload).await;
                                quick_links.refetch();
                            }
                        });
                    };

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
                                    on:click=reset_both
                                >
                                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.2"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>
                                        <path d="M3 3v5h5"/>
                                    </svg>
                                </button>
                            </div>
                            {svc_row}
                            {link_row}
                        </div>
                    }
                }).collect_view();

                view! { <div>{sections}</div> }.into_any()
            }}
        </Transition>
    }
}
