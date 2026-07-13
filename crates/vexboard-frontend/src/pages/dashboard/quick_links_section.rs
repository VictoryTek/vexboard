use leptos::either::EitherOf3;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::quick_link_card::{QuickLinkCard, QuickLinkData};
use crate::components::quick_link_modal::QuickLinkFormData;
use crate::CurrentUser;

use super::{fetch_quick_links, reorder_quick_links, QuickLinkResponse, SortMode};

#[component]
pub(super) fn QuickLinksSection(
    quick_links: LocalResource<Vec<QuickLinkResponse>>,
    sort_mode: ReadSignal<SortMode>,
    drag_src_idx: RwSignal<Option<usize>>,
    drag_over_idx: RwSignal<Option<usize>>,
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
                    // Group mode is rendered entirely by the sibling `GroupSection`
                    // component, which interleaves this group's quick links below its
                    // services within one shared container per group.
                    EitherOf3::B(())
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
                            <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:0.75rem;">
                                {cards}
                            </div>
                        </div>
                    })
                }
            })}
        </Suspense>
    }
}
