use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Debug, Clone, serde::Deserialize)]
struct GroupEntry {
    id: i64,
    name: String,
    #[allow(dead_code)]
    icon: Option<String>,
    sort_order: i64,
}

async fn fetch_groups_internal() -> Vec<GroupEntry> {
    let Ok(resp) = gloo_net::http::Request::get("/api/v1/groups").send().await else {
        return Vec::new();
    };
    if !resp.ok() {
        return Vec::new();
    }
    resp.json::<Vec<GroupEntry>>().await.unwrap_or_default()
}

#[component]
pub fn GroupsModal(
    #[prop(into)] visible: Signal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_saved: Callback<()>,
) -> impl IntoView {
    let groups = LocalResource::new(fetch_groups_internal);

    // id of the group currently being renamed
    let editing_id: RwSignal<Option<i64>> = RwSignal::new(None);
    let edit_name: RwSignal<String> = RwSignal::new(String::new());

    // new-group form
    let (new_name, set_new_name) = signal(String::new());

    let do_create = move || {
        let name = new_name.get_untracked().trim().to_string();
        if name.is_empty() {
            return;
        }
        spawn_local(async move {
            let body = serde_json::json!({ "name": name, "sort_order": 0 });
            if let Ok(req) = gloo_net::http::Request::post("/api/v1/groups").json(&body) {
                let _ = req.send().await;
            }
            set_new_name.set(String::new());
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
        let on_saved = on_saved;
        spawn_local(async move {
            let body = serde_json::json!({ "name": name });
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
                                        let is_first = i == 0;
                                        let is_last = i == len - 1;
                                        let is_editing = move || editing_id.get() == Some(id);

                                        view! {
                                            <div style="display:flex; align-items:center; gap:0.5rem; \
                                                        padding:0.5rem 0.625rem; border-radius:0.5rem; \
                                                        background:var(--color-bg-primary); border:1px solid var(--color-border);">

                                                // Reorder buttons
                                                <div style="display:flex; flex-direction:column; gap:1px; flex-shrink:0;">
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
                                                    style="background:none; border:none; cursor:pointer; \
                                                           color:var(--color-text-muted); padding:0.25rem; \
                                                           border-radius:0.375rem; flex-shrink:0;"
                                                    title="Rename"
                                                    on:click=move |_| {
                                                        edit_name.set(name_for_rename.clone());
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
                                                    style="background:none; border:none; cursor:pointer; \
                                                           color:var(--color-text-muted); padding:0.25rem; \
                                                           border-radius:0.375rem; flex-shrink:0;"
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
                        <div style="display:flex; gap:0.5rem;">
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
