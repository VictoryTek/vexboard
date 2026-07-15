use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::pages::dashboard::GroupResponse;

const PALETTE: &[(&str, &str)] = &[
    ("Blue", "#3b82f6"),
    ("Purple", "#8b5cf6"),
    ("Green", "#22c55e"),
    ("Orange", "#f97316"),
    ("Red", "#ef4444"),
    ("Pink", "#ec4899"),
    ("Yellow", "#eab308"),
    ("Teal", "#14b8a6"),
    ("Gray", "#6b7280"),
];

const DEFAULT_COLOR: &str = "#3b82f6";

#[component]
fn ColorSwatches(
    #[prop(into)] selected: Signal<String>,
    #[prop(into)] on_select: Callback<String>,
) -> impl IntoView {
    view! {
        <div style="display:flex; flex-wrap:wrap; gap:5px; margin-top:0.375rem;">
            {PALETTE.iter().map(|(label, hex)| {
                let hex_str = hex.to_string();
                let hex_for_click = hex_str.clone();
                let hex_for_style = hex_str.clone();
                view! {
                    <button
                        type="button"
                        title=*label
                        style=move || {
                            let is_selected = selected.get() == hex_for_style;
                            format!(
                                "width:20px; height:20px; border-radius:50%; background:{hex_for_style}; \
                                 border:{}; cursor:pointer; padding:0; flex-shrink:0; \
                                 box-shadow:{};",
                                if is_selected { "2px solid var(--color-text-primary)" } else { "2px solid transparent" },
                                if is_selected { "0 0 0 1px var(--color-bg-surface)" } else { "none" },
                            )
                        }
                        on:click=move |_| on_select.run(hex_for_click.clone())
                    />
                }
            }).collect_view()}
        </div>
    }
}

#[component]
pub fn GroupsModal(
    #[prop(into)] visible: Signal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_saved: Callback<()>,
    groups: LocalResource<Vec<GroupResponse>>,
) -> impl IntoView {
    // Re-fetch whenever the modal opens so it always reflects the same
    // server state as the rest of the dashboard, regardless of how stale
    // the shared resource's last-fetched snapshot might be.
    Effect::new(move |_| {
        if visible.get() {
            groups.refetch();
        }
    });

    // id of the group currently being renamed
    let editing_id: RwSignal<Option<i64>> = RwSignal::new(None);
    let edit_name: RwSignal<String> = RwSignal::new(String::new());
    let edit_color: RwSignal<String> = RwSignal::new(DEFAULT_COLOR.to_string());

    // new-group form
    let (new_name, set_new_name) = signal(String::new());
    let (new_color, set_new_color) = signal(DEFAULT_COLOR.to_string());

    let do_create = move || {
        let name = new_name.get_untracked().trim().to_string();
        if name.is_empty() {
            return;
        }
        let color = new_color.get_untracked();
        let next_order = groups
            .get_untracked()
            .unwrap_or_default()
            .iter()
            .map(|g| g.sort_order)
            .max()
            .map_or(0, |max| max + 1);
        spawn_local(async move {
            let body =
                serde_json::json!({ "name": name, "color": color, "sort_order": next_order });
            if let Ok(req) = gloo_net::http::Request::post("/api/v1/groups").json(&body) {
                let _ = req.send().await;
            }
            set_new_name.set(String::new());
            set_new_color.set(DEFAULT_COLOR.to_string());
            groups.refetch();
            on_saved.run(());
        });
    };

    let do_rename = move |id: i64| {
        let name = edit_name.get_untracked().trim().to_string();
        if name.is_empty() {
            editing_id.set(None);
            return;
        }
        let color = edit_color.get_untracked();
        let on_saved = on_saved;
        spawn_local(async move {
            let body = serde_json::json!({ "name": name, "color": color });
            if let Ok(req) =
                gloo_net::http::Request::put(&format!("/api/v1/groups/{id}")).json(&body)
            {
                let _ = req.send().await;
            }
            editing_id.set(None);
            groups.refetch();
            on_saved.run(());
        });
    };

    let do_delete = move |id: i64| {
        let on_saved = on_saved;
        spawn_local(async move {
            let _ = gloo_net::http::Request::delete(&format!("/api/v1/groups/{id}"))
                .send()
                .await;
            groups.refetch();
            on_saved.run(());
        });
    };

    let do_move = move |id: i64, direction: i32| {
        let list = groups.get().unwrap_or_default();
        let pos = list.iter().position(|g| g.id == id);
        let Some(idx) = pos else { return };
        let swap_idx = (idx as i32 + direction) as usize;
        if swap_idx >= list.len() {
            return;
        }
        let a = list[idx].clone();
        let b = list[swap_idx].clone();
        let new_a_order = b.sort_order;
        let new_b_order = a.sort_order;
        let on_saved = on_saved;
        spawn_local(async move {
            let body_a = serde_json::json!({ "sort_order": new_a_order });
            let body_b = serde_json::json!({ "sort_order": new_b_order });
            if let Ok(req) =
                gloo_net::http::Request::put(&format!("/api/v1/groups/{}", a.id)).json(&body_a)
            {
                let _ = req.send().await;
            }
            if let Ok(req) =
                gloo_net::http::Request::put(&format!("/api/v1/groups/{}", b.id)).json(&body_b)
            {
                let _ = req.send().await;
            }
            groups.refetch();
            on_saved.run(());
        });
    };

    view! {
        <Show when=move || visible.get()>
            <div style="position:fixed; inset:0; z-index:50; display:flex; align-items:center; justify-content:center;">
                // Backdrop
                <div
                    style="position:absolute; inset:0; background:rgba(0,0,0,0.6); backdrop-filter:blur(4px);"
                    on:click=move |_| on_close.run(())
                ></div>
                // Panel
                <div style="position:relative; background:var(--color-bg-surface); border:1px solid var(--color-border); \
                             border-radius:1rem; box-shadow:0 25px 50px rgba(0,0,0,0.5); \
                             width:100%; max-width:460px; padding:1.5rem; margin:1rem;">
                    <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:1.25rem;">
                        <h2 style="font-size:1rem; font-weight:600; margin:0;">"Manage Groups"</h2>
                        <button
                            style="background:none; border:none; cursor:pointer; color:var(--color-text-muted); \
                                   padding:0.25rem; border-radius:0.375rem; line-height:1;"
                            on:click=move |_| on_close.run(())
                        >
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2"
                                 stroke-linecap="round" stroke-linejoin="round">
                                <line x1="18" y1="6" x2="6" y2="18"/>
                                <line x1="6" y1="6" x2="18" y2="18"/>
                            </svg>
                        </button>
                    </div>

                    // Group list
                    <Suspense fallback=|| ()>
                        {move || {
                            let list = groups.get().unwrap_or_default();
                            let len = list.len();
                            if list.is_empty() {
                                return leptos::either::Either::Left(view! {
                                    <p style="font-size:0.8rem; color:var(--color-text-muted); margin:0 0 1rem;">
                                        "No groups yet. Create one below."
                                    </p>
                                });
                            }
                            leptos::either::Either::Right(view! {
                                <div style="display:flex; flex-direction:column; gap:0.375rem; margin-bottom:1rem;">
                                    {list.into_iter().enumerate().map(|(i, g)| {
                                        let id = g.id;
                                        let name_str = g.name.clone();
                                        let name_for_rename = name_str.clone();
                                        let group_color = g.color.clone()
                                            .unwrap_or_else(|| DEFAULT_COLOR.to_string());
                                        let group_color_swatch = group_color.clone();
                                        let is_first = i == 0;
                                        let is_last = i == len - 1;
                                        let is_editing = move || editing_id.get() == Some(id);

                                        view! {
                                            <div style="display:flex; align-items:flex-start; gap:0.5rem; \
                                                        padding:0.5rem 0.625rem; border-radius:0.5rem; \
                                                        background:var(--color-bg-primary); border:1px solid var(--color-border);">

                                                // Reorder buttons
                                                <div style="display:flex; flex-direction:column; gap:1px; flex-shrink:0; padding-top:2px;">
                                                    <button
                                                        style=move || format!(
                                                            "background:none; border:none; cursor:{}; padding:1px 3px; \
                                                             color:{}; line-height:1; border-radius:3px;",
                                                            if is_first { "default" } else { "pointer" },
                                                            if is_first { "var(--color-border)" } else { "var(--color-text-muted)" }
                                                        )
                                                        disabled=is_first
                                                        on:click=move |_| do_move(id, -1)
                                                    >
                                                        <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                                                             stroke="currentColor" stroke-width="2.5"
                                                             stroke-linecap="round" stroke-linejoin="round">
                                                            <polyline points="18 15 12 9 6 15"/>
                                                        </svg>
                                                    </button>
                                                    <button
                                                        style=move || format!(
                                                            "background:none; border:none; cursor:{}; padding:1px 3px; \
                                                             color:{}; line-height:1; border-radius:3px;",
                                                            if is_last { "default" } else { "pointer" },
                                                            if is_last { "var(--color-border)" } else { "var(--color-text-muted)" }
                                                        )
                                                        disabled=is_last
                                                        on:click=move |_| do_move(id, 1)
                                                    >
                                                        <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                                                             stroke="currentColor" stroke-width="2.5"
                                                             stroke-linecap="round" stroke-linejoin="round">
                                                            <polyline points="6 9 12 15 18 9"/>
                                                        </svg>
                                                    </button>
                                                </div>

                                                // Color dot (non-editing) or swatch picker (editing)
                                                <div style="flex-shrink:0; display:flex; align-items:flex-start; padding-top:3px;">
                                                    {move || if is_editing() {
                                                        leptos::either::Either::Left(view! {
                                                            <ColorSwatches
                                                                selected=Signal::derive(move || edit_color.get())
                                                                on_select=Callback::new(move |c: String| edit_color.set(c))
                                                            />
                                                        })
                                                    } else {
                                                        leptos::either::Either::Right(view! {
                                                            <div style=format!(
                                                                "width:12px; height:12px; border-radius:50%; \
                                                                 background:{group_color_swatch}; margin-top:1px; flex-shrink:0;"
                                                            )/>
                                                        })
                                                    }}
                                                </div>

                                                // Name — inline edit or label
                                                <div style="flex:1; min-width:0;">
                                                    {move || if is_editing() {
                                                        leptos::either::Either::Left(view! {
                                                            <input
                                                                type="text"
                                                                class="form-input"
                                                                style="padding:0.25rem 0.5rem; font-size:0.85rem;"
                                                                prop:value=move || edit_name.get()
                                                                on:input=move |ev| edit_name.set(leptos::prelude::event_target_value(&ev))
                                                                on:blur=move |_| do_rename(id)
                                                                on:keydown=move |ev| {
                                                                    use wasm_bindgen::JsCast;
                                                                    if let Some(ke) = ev.dyn_ref::<web_sys::KeyboardEvent>() {
                                                                        if ke.key() == "Enter" { do_rename(id); }
                                                                        if ke.key() == "Escape" { editing_id.set(None); }
                                                                    }
                                                                }
                                                            />
                                                        })
                                                    } else {
                                                        leptos::either::Either::Right(view! {
                                                            <span style="font-size:0.875rem; font-weight:500; color:var(--color-text-primary);">
                                                                {name_str.clone()}
                                                            </span>
                                                        })
                                                    }}
                                                </div>

                                                // Rename button
                                                <button
                                                    style=move || format!(
                                                        "background:none; border:none; cursor:pointer; \
                                                         color:var(--color-text-muted); padding:0.25rem; \
                                                         border-radius:0.375rem; flex-shrink:0; \
                                                         display:{};",
                                                        if is_editing() { "none" } else { "inline-flex" }
                                                    )
                                                    title="Rename"
                                                    on:click=move |_| {
                                                        edit_name.set(name_for_rename.clone());
                                                        edit_color.set(group_color.clone());
                                                        editing_id.set(Some(id));
                                                    }
                                                >
                                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                                         stroke="currentColor" stroke-width="2"
                                                         stroke-linecap="round" stroke-linejoin="round">
                                                        <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                                                        <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                                    </svg>
                                                </button>

                                                // Delete button
                                                <button
                                                    style=move || format!(
                                                        "background:none; border:none; cursor:pointer; \
                                                         color:var(--color-text-muted); padding:0.25rem; \
                                                         border-radius:0.375rem; flex-shrink:0; \
                                                         display:{};",
                                                        if is_editing() { "none" } else { "inline-flex" }
                                                    )
                                                    title="Delete group"
                                                    on:click=move |_| do_delete(id)
                                                >
                                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                                         stroke="currentColor" stroke-width="2"
                                                         stroke-linecap="round" stroke-linejoin="round">
                                                        <polyline points="3 6 5 6 21 6"/>
                                                        <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
                                                        <path d="M10 11v6"/>
                                                        <path d="M14 11v6"/>
                                                        <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                                                    </svg>
                                                </button>

                                                // Save button
                                                <button
                                                    style=move || format!(
                                                        "background:none; border:none; cursor:pointer; \
                                                         color:var(--color-text-muted); padding:0.25rem; \
                                                         border-radius:0.375rem; flex-shrink:0; \
                                                         display:{};",
                                                        if is_editing() { "inline-flex" } else { "none" }
                                                    )
                                                    title="Save"
                                                    on:click=move |_| do_rename(id)
                                                >
                                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                                         stroke="currentColor" stroke-width="2"
                                                         stroke-linecap="round" stroke-linejoin="round">
                                                        <polyline points="20 6 9 17 4 12"/>
                                                    </svg>
                                                </button>

                                                // Cancel button
                                                <button
                                                    style=move || format!(
                                                        "background:none; border:none; cursor:pointer; \
                                                         color:var(--color-text-muted); padding:0.25rem; \
                                                         border-radius:0.375rem; flex-shrink:0; \
                                                         display:{};",
                                                        if is_editing() { "inline-flex" } else { "none" }
                                                    )
                                                    title="Cancel"
                                                    on:click=move |_| editing_id.set(None)
                                                >
                                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                                         stroke="currentColor" stroke-width="2"
                                                         stroke-linecap="round" stroke-linejoin="round">
                                                        <line x1="18" y1="6" x2="6" y2="18"/>
                                                        <line x1="6" y1="6" x2="18" y2="18"/>
                                                    </svg>
                                                </button>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            })
                        }}
                    </Suspense>

                    // Create new group
                    <div style="border-top:1px solid var(--color-border); padding-top:1rem;">
                        <p style="font-size:0.72rem; font-weight:600; text-transform:uppercase; \
                                   letter-spacing:0.06em; color:var(--color-text-muted); margin:0 0 0.5rem;">
                            "New Group"
                        </p>
                        <div style="display:flex; gap:0.5rem; align-items:flex-start;">
                            <div style="flex:1; display:flex; flex-direction:column; gap:0.375rem;">
                                <input
                                    type="text"
                                    class="form-input"
                                    placeholder="Group name"
                                    prop:value=move || new_name.get()
                                    on:input=move |ev| set_new_name.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        if let Some(ke) = ev.dyn_ref::<web_sys::KeyboardEvent>() {
                                            if ke.key() == "Enter" { do_create(); }
                                        }
                                    }
                                />
                                <ColorSwatches
                                    selected=Signal::derive(move || new_color.get())
                                    on_select=Callback::new(move |c: String| set_new_color.set(c))
                                />
                            </div>
                            <button
                                class="btn-primary"
                                style="flex-shrink:0; white-space:nowrap;"
                                on:click=move |_| do_create()
                            >
                                "+ Create"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}
