use leptos::either::Either;
use leptos::prelude::*;

use crate::components::icon_picker::IconPicker;
use crate::components::modal_edit::GroupItem;

#[derive(Debug, Clone)]
pub struct QuickLinkFormData {
    pub title: String,
    pub url: String,
    pub icon: String,
    pub description: String,
    pub group_id: Option<i64>,
}

fn extract_favicon_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let after_scheme = url.find("://")? + 3;
    let rest = &url[after_scheme..];
    let host_end = rest.find('/').unwrap_or(rest.len());
    let scheme = &url[..after_scheme - 3];
    let host = &rest[..host_end];
    if host.is_empty() {
        return None;
    }
    Some(format!("{}://{}/favicon.ico", scheme, host))
}

#[component]
pub fn QuickLinkModal(
    #[prop(into)] visible: Signal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_save: Callback<QuickLinkFormData>,
    #[prop(default = "Add Quick Link")] title: &'static str,
    #[prop(optional)] initial: Option<QuickLinkFormData>,
    #[prop(default = vec![])] groups: Vec<GroupItem>,
) -> impl IntoView {
    let initial = initial.unwrap_or(QuickLinkFormData {
        title: String::new(),
        url: String::new(),
        icon: String::new(),
        description: String::new(),
        group_id: None,
    });

    let (name, set_name) = signal(initial.title);
    let (url, set_url) = signal(initial.url);
    let (icon, set_icon) = signal(initial.icon);
    let (icon_auto, set_icon_auto) = signal(true);
    let (desc, set_desc) = signal(initial.description);
    let (selected_group_id, set_selected_group_id) = signal(initial.group_id);

    view! {
        <Show when=move || visible.get()>
            <div style="position:fixed; inset:0; z-index:50; display:flex; align-items:center; justify-content:center;">
                <div
                    style="position:absolute; inset:0; background:rgba(0,0,0,0.6); backdrop-filter:blur(4px);"
                    on:click=move |_| on_close.run(())
                ></div>
                <div style="position:relative; background:var(--color-bg-surface); border:1px solid var(--color-border); \
                             border-radius:1rem; box-shadow:0 25px 50px rgba(0,0,0,0.5); \
                             width:100%; max-width:420px; padding:1.5rem; margin:1rem;">
                    <h2 style="font-size:1rem; font-weight:600; margin:0 0 1.25rem;">{title}</h2>
                    <div style="display:flex; flex-direction:column; gap:0.875rem;">
                        <div>
                            <label class="form-label">"Title"</label>
                            <input type="text" class="form-input" required=true
                                prop:value=move || name.get()
                                on:input=move |ev| set_name.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="form-label">"URL"</label>
                            <input type="url" class="form-input"
                                prop:value=move || url.get()
                                on:input=move |ev| {
                                    let val = event_target_value(&ev);
                                    if icon.get().is_empty() || icon_auto.get() {
                                        match extract_favicon_url(&val) {
                                            Some(fav) => { set_icon.set(fav); set_icon_auto.set(true); }
                                            None => { if icon_auto.get() { set_icon.set(String::new()); } }
                                        }
                                    }
                                    set_url.set(val);
                                }
                            />
                        </div>
                        <div>
                            <label class="form-label">"Description"</label>
                            <input type="text" class="form-input"
                                prop:value=move || desc.get()
                                on:input=move |ev| set_desc.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="form-label">"Icon"</label>
                            <div style="display:flex; align-items:center; gap:0.5rem;">
                                <div style="width:2.25rem;height:2.25rem;flex-shrink:0;border-radius:0.5rem;\
                                             background:var(--color-bg-primary);border:1px solid var(--color-border);\
                                             display:flex;align-items:center;justify-content:center;overflow:hidden;">
                                    {move || {
                                        let ic = icon.get();
                                        if ic.starts_with("http://") || ic.starts_with("https://") {
                                            Either::Left(view! {
                                                <img src={ic} alt=""
                                                    style="width:100%;height:100%;object-fit:contain;padding:4px;"
                                                    on:error=move |ev| {
                                                        use wasm_bindgen::JsCast;
                                                        if let Some(t) = ev.target() {
                                                            if let Ok(el) = t.dyn_into::<web_sys::HtmlElement>() {
                                                                let _ = el.style().set_property("opacity", "0.15");
                                                            }
                                                        }
                                                    }
                                                />
                                            })
                                        } else {
                                            Either::Right(view! {
                                                <span style="font-size:1rem;line-height:1;">{ic}</span>
                                            })
                                        }
                                    }}
                                </div>
                                <input type="text" class="form-input"
                                    placeholder="auto-detected from URL, or enter emoji"
                                    prop:value=move || icon.get()
                                    on:input=move |ev| {
                                        set_icon_auto.set(false);
                                        set_icon.set(event_target_value(&ev));
                                    }
                                />
                            </div>
                            <IconPicker on_select=move |url: String| {
                                set_icon_auto.set(false);
                                set_icon.set(url);
                            } />
                        </div>
                        // Group selector — only rendered when groups are available
                        {if !groups.is_empty() {
                            let groups = groups.clone();
                            Either::Left(view! {
                                <div>
                                    <label class="form-label">"Group"</label>
                                    <select
                                        class="form-input"
                                        on:change=move |ev| {
                                            let val = event_target_value(&ev);
                                            if val.is_empty() {
                                                set_selected_group_id.set(None);
                                            } else if let Ok(id) = val.parse::<i64>() {
                                                set_selected_group_id.set(Some(id));
                                            }
                                        }
                                    >
                                        <option value="" selected=move || selected_group_id.get().is_none()>
                                            "— No group —"
                                        </option>
                                        {groups.into_iter().map(|g| {
                                            let gid = g.id;
                                            view! {
                                                <option
                                                    value={g.id.to_string()}
                                                    selected=move || selected_group_id.get() == Some(gid)
                                                >
                                                    {g.name}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                </div>
                            })
                        } else {
                            Either::Right(())
                        }}
                    </div>
                    <div style="display:flex; justify-content:flex-end; gap:0.5rem; margin-top:1.5rem;">
                        <button class="btn-secondary" on:click=move |_| on_close.run(())>
                            "Cancel"
                        </button>
                        <button class="btn-primary" on:click=move |_| {
                            on_save.run(QuickLinkFormData {
                                title: name.get(),
                                url: url.get(),
                                icon: icon.get(),
                                description: desc.get(),
                                group_id: selected_group_id.get(),
                            });
                        }>
                            "Save"
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
