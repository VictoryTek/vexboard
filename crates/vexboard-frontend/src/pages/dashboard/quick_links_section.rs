use leptos::either::EitherOf3;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::quick_link_card::{QuickLinkCard, QuickLinkData};
use crate::components::quick_link_modal::QuickLinkFormData;
use crate::CurrentUser;

use super::{
    fetch_quick_links, reorder_quick_links, QuickLinkGroupResponse, QuickLinkResponse, SortMode,
};

#[component]
pub(super) fn QuickLinksSection(
    quick_links: LocalResource<Vec<QuickLinkResponse>>,
    groups: LocalResource<Vec<QuickLinkGroupResponse>>,
    sort_mode: ReadSignal<SortMode>,
    drag_src_idx: RwSignal<Option<usize>>,
    drag_over_idx: RwSignal<Option<usize>>,
    section_drag_src: RwSignal<Option<(String, usize)>>,
    section_drag_over: RwSignal<Option<(String, usize)>>,
    edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>>,
) -> impl IntoView {
    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    let render_card = move |link: QuickLinkResponse| {
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
        <Suspense fallback=|| ()>
            {move || quick_links.get().map(|links| {
                if links.is_empty() {
                    return EitherOf3::A(());
                }

                let header = view! {
                    <div style="display:flex; align-items:center; gap:0.5rem; margin-bottom:0.75rem;">
                        <h2 style="font-size:0.8rem; font-weight:600; text-transform:uppercase; \
                                    letter-spacing:0.08em; color:var(--color-text-muted); margin:0;">
                            "Quick Links"
                        </h2>
                        <div style="flex:1;"></div>
                    </div>
                };

                if sort_mode.get() == SortMode::Group {
                    let group_list = groups.get().unwrap_or_default();
                    let known_ids: std::collections::HashSet<i64> =
                        group_list.iter().map(|g| g.id).collect();

                    type Section = (String, String, String, String, String, Vec<QuickLinkResponse>);
                    let mut sections_data: Vec<Section> = group_list.iter().filter_map(|grp| {
                        let gid = grp.id;
                        let mut members: Vec<QuickLinkResponse> = links.iter()
                            .filter(|l| l.group_id == Some(gid))
                            .cloned()
                            .collect();
                        if members.is_empty() { return None; }
                        members.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
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

                    let mut ungrouped: Vec<QuickLinkResponse> = links.iter()
                        .filter(|l| l.group_id.is_none_or(|gid| !known_ids.contains(&gid)))
                        .cloned()
                        .collect();
                    if !ungrouped.is_empty() {
                        ungrouped.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
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
                        let member_ids: Vec<i64> = members.iter().map(|l| l.id).collect();
                        let reset_payload: Vec<(i64, i64)> = {
                            let mut sorted = members.clone();
                            sorted.sort_by_key(|a| a.title.to_lowercase());
                            sorted.iter().enumerate().map(|(i, l)| (l.id, i as i64)).collect()
                        };
                        let members_with_idx: Vec<(usize, QuickLinkResponse)> =
                            members.into_iter().enumerate().collect();
                        let cards = members_with_idx.into_iter().map(|(idx, link)| {
                            let card = render_card(link);
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
                                                let _ = reorder_quick_links(payload).await;
                                                quick_links.refetch();
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
                                <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:0.75rem; max-width:1200px;">
                                    {cards}
                                </div>
                            </div>
                        }
                    }).collect_view();

                    EitherOf3::B(view! {
                        <div style="margin-top:2rem;">
                            {header}
                            <div>{sections}</div>
                        </div>
                    })
                } else {
                    let mut links = links;
                    links.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
                    let links_with_idx: Vec<(usize, QuickLinkResponse)> =
                        links.into_iter().enumerate().collect();
                    let cards = links_with_idx.into_iter().map(|(idx, link)| {
                        let card = render_card(link);
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
                                                if let Ok(mut current) = fetch_quick_links().await {
                                                    let item = current.remove(src_i);
                                                    current.insert(dst_i, item);
                                                    let payload: Vec<_> = current.iter()
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
                                    drag_src_idx.set(None);
                                    drag_over_idx.set(None);
                                }
                            >
                                {card}
                            </div>
                        }
                    }).collect_view();

                    EitherOf3::C(view! {
                        <div style="margin-top:2rem;">
                            {header}
                            <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:0.75rem; max-width:1200px;">
                                {cards}
                            </div>
                        </div>
                    })
                }
            })}
        </Suspense>
    }
}
