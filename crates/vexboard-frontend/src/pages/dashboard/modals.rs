use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::modal_edit::{EditFormData, EditModal};
use crate::components::modal_groups::GroupsModal;
use crate::components::quick_link_modal::{QuickLinkFormData, QuickLinkModal};

use super::{resolve_groups, GroupResponse, QuickLinkResponse, ServiceResponse};

#[component]
pub(super) fn DashboardModals(
    services: LocalResource<Vec<ServiceResponse>>,
    quick_links: LocalResource<Vec<QuickLinkResponse>>,
    groups: LocalResource<Vec<GroupResponse>>,
    show_modal: RwSignal<bool>,
    show_add_link_modal: RwSignal<bool>,
    show_groups_modal: RwSignal<bool>,
    edit_target: RwSignal<Option<(i64, EditFormData)>>,
    edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>>,
) -> impl IntoView {
    let on_save = Callback::new(move |data: EditFormData| {
        spawn_local(async move {
            let body = serde_json::json!({
                "display_name": data.display_name,
                "description": if data.description.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.description) },
                "url": if data.url.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.url) },
                "icon": if data.icon.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(data.icon) },
                "group_id": data.group_id,
                "probe_enabled": data.probe_enabled,
                "probe_interval": data.probe_interval,
            });
            if let Ok(req) = gloo_net::http::Request::post("/api/v1/services").json(&body) {
                let _ = req.send().await;
            }
            show_modal.set(false);
            services.refetch();
            // The backend fires an immediate probe; wait briefly then refetch so
            // the status dot reflects the probe result rather than "unknown".
            TimeoutFuture::new(1_500).await;
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
            show_add_link_modal.set(false);
            quick_links.refetch();
        });
    });

    view! {
        // Group management modal
        <GroupsModal
            visible=show_groups_modal
            on_close=Callback::new(move |_| show_groups_modal.set(false))
            on_saved=Callback::new(move |_| { groups.refetch(); services.refetch(); })
        />

        // Add service modal — reactive wrapper so groups prop updates when resource loads
        {move || view! {
            <EditModal
                visible=show_modal
                on_close=Callback::new(move |_| show_modal.set(false))
                on_save=on_save
                groups=resolve_groups(&groups)
            />
        }}

        // Add quick link modal
        <QuickLinkModal
            visible=show_add_link_modal
            on_close=Callback::new(move |_| show_add_link_modal.set(false))
            on_save=on_save_link
        />

        // Edit service modal
        {move || edit_target.get().map(|(id, initial)| {
            let group_items = resolve_groups(&groups);
            let (show_edit, set_show_edit) = signal(true);
            let on_edit_save = Callback::new(move |data: EditFormData| {
                spawn_local(async move {
                    let body = serde_json::json!({
                        "display_name": data.display_name,
                        "description": data.description,
                        "url": data.url,
                        "icon": data.icon,
                        "group_id": data.group_id,
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
                    groups=group_items
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
    }
}
