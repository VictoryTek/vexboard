use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::modal_edit::{EditFormData, EditModal};

#[derive(Debug, Clone, serde::Deserialize)]
struct DiscoveredUnitFe {
    unit_name: String,
    description: String,
    #[allow(dead_code)]
    active_state: String,
    #[allow(dead_code)]
    sub_state: String,
    source: String,
    url_hint: Option<String>,
}

impl DiscoveredUnitFe {
    fn display_name(&self) -> String {
        self.unit_name.trim_end_matches(".service").to_string()
    }

    fn source_label(&self) -> &str {
        match self.source.as_str() {
            "docker" => "Docker",
            "podman" => "Podman",
            _ => "Systemd",
        }
    }

    fn source_color(&self) -> &str {
        match self.source.as_str() {
            "docker" => "#0db7ed",
            "podman" => "#892ca0",
            _ => "var(--color-accent)",
        }
    }
}

async fn fetch_discovered_units() -> Vec<DiscoveredUnitFe> {
    let Ok(resp) = gloo_net::http::Request::get("/api/v1/discovery")
        .send()
        .await
    else {
        return Vec::new();
    };
    if !resp.ok() {
        return Vec::new();
    }
    resp.json::<Vec<DiscoveredUnitFe>>().await.unwrap_or_default()
}

#[component]
pub fn DiscoveryPanel(#[prop(into)] on_added: Callback<()>) -> impl IntoView {
    let units = LocalResource::new(fetch_discovered_units);
    let (editing, set_editing) = signal::<Option<DiscoveredUnitFe>>(None);

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
            set_editing.set(None);
            units.refetch();
            on_added.run(());
        });
    });

    view! {
        // Modal mounts fresh each time editing is Some — initial data is correct on every open
        {move || editing.get().map(|unit| {
            let init = EditFormData {
                display_name: unit.display_name(),
                description: unit.description.clone(),
                url: unit.url_hint.clone().unwrap_or_default(),
                icon: String::new(),
                group_id: None,
                probe_enabled: true,
                probe_interval: 30,
            };
            view! {
                <EditModal
                    visible=Signal::derive(|| true)
                    on_close=Callback::new(move |_| set_editing.set(None))
                    on_save=on_save
                    title="Add Discovered Service"
                    initial=init
                />
            }
        })}

        // Panel — only visible when there are discovered units
        {move || {
            let us = units.get().unwrap_or_default();
            if us.is_empty() {
                return None;
            }
            Some(view! {
                <div style="margin-top:2rem;">
                    <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:0.875rem;">
                        <h2 style="font-size:0.75rem; font-weight:700; letter-spacing:0.07em; text-transform:uppercase; color:var(--color-text-muted); margin:0;">
                            "Discovered Services"
                        </h2>
                        <button
                            style="font-size:0.75rem; color:var(--color-text-muted); background:none; border:none; cursor:pointer; padding:0.2rem 0.5rem; border-radius:0.375rem;"
                            on:click=move |_| {
                                spawn_local(async move {
                                    let _ = gloo_net::http::Request::post("/api/v1/discovery/refresh")
                                        .send()
                                        .await;
                                    units.refetch();
                                });
                            }
                        >
                            "↻ Refresh"
                        </button>
                    </div>
                    <div style="display:grid; grid-template-columns:repeat(auto-fill,minmax(280px,1fr)); gap:0.625rem;">
                        {us.into_iter().map(|unit| {
                            let color = unit.source_color().to_string();
                            let label = unit.source_label().to_string();
                            let name = unit.display_name();
                            let desc = unit.description.clone();
                            let url_hint = unit.url_hint.clone();
                            let unit_c = unit.clone();
                            view! {
                                <div class="service-card" style="opacity:0.88;">
                                    <div style="display:flex; align-items:flex-start; justify-content:space-between; gap:0.75rem;">
                                        <div style="min-width:0; flex:1;">
                                            <span style=format!(
                                                "display:inline-block; font-size:0.6rem; font-weight:700; \
                                                 letter-spacing:0.06em; text-transform:uppercase; \
                                                 color:{color}; background:{color}22; \
                                                 border-radius:0.25rem; padding:0.1rem 0.4rem; margin-bottom:0.3rem;"
                                            )>{label}</span>
                                            <p style="font-size:0.875rem; font-weight:600; color:var(--color-text-primary); margin:0 0 0.15rem; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                                                {name}
                                            </p>
                                            <p style="font-size:0.75rem; color:var(--color-text-muted); margin:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                                                {desc}
                                            </p>
                                            {url_hint.map(|u| view! {
                                                <p style="font-size:0.7rem; color:var(--color-accent); margin:0.2rem 0 0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                                                    {u}
                                                </p>
                                            })}
                                        </div>
                                        <button
                                            class="btn-primary"
                                            style="flex-shrink:0; padding:0.3rem 0.75rem; font-size:0.75rem;"
                                            on:click=move |_| set_editing.set(Some(unit_c.clone()))
                                        >
                                            "Add"
                                        </button>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </div>
            })
        }}
    }
}
