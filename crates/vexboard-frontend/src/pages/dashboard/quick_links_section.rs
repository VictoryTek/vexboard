use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::quick_link_card::{QuickLinkCard, QuickLinkData};
use crate::components::quick_link_modal::QuickLinkFormData;
use crate::CurrentUser;

use super::QuickLinkResponse;

#[component]
pub(super) fn QuickLinksSection(
    quick_links: LocalResource<Vec<QuickLinkResponse>>,
    edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>>,
) -> impl IntoView {
    let current_user = use_context::<RwSignal<Option<CurrentUser>>>();
    let is_admin = move || {
        current_user
            .and_then(|u| u.get())
            .map(|u| u.is_admin())
            .unwrap_or(false)
    };

    view! {
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
                                    let (on_delete, on_edit) = if is_admin() {
                                        let cb_delete = Callback::new(move |_: i64| {
                                            spawn_local(async move {
                                                let _ = gloo_net::http::Request::delete(
                                                    &format!("/api/v1/quick-links/{id}")
                                                ).send().await;
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
                                }).collect_view()}
                            </div>
                        </div>
                    })
                }
            })}
        </Suspense>
    }
}
