use gloo_net::http::Request;
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsCast;

const DEFAULT_CDN: &str = "https://cdn.jsdelivr.net/gh/selfhst/icons@main";

#[derive(Deserialize, Clone, Debug)]
struct IconEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Reference")]
    reference: String,
}

async fn load_manifest() -> Vec<IconEntry> {
    match Request::get("/icons-index.json").send().await {
        Ok(resp) => resp.json::<Vec<IconEntry>>().await.unwrap_or_default(),
        Err(_) => vec![],
    }
}

async fn load_cdn_base() -> String {
    let Ok(resp) = Request::get("/api/v1/config/public").send().await else {
        return DEFAULT_CDN.to_string();
    };
    let Ok(val) = resp.json::<serde_json::Value>().await else {
        return DEFAULT_CDN.to_string();
    };
    val["icon_cdn_base"]
        .as_str()
        .unwrap_or(DEFAULT_CDN)
        .to_string()
}

/// Inline icon browser that searches the selfhst/icons manifest.
/// Calls `on_select` with the full CDN SVG URL when an icon is picked.
#[component]
pub fn IconPicker(#[prop(into)] on_select: Callback<String>) -> impl IntoView {
    let (open, set_open) = signal(false);
    let (query, set_query) = signal(String::new());

    let manifest = LocalResource::new(load_manifest);
    let cdn_base = LocalResource::new(load_cdn_base);

    view! {
        <div style="position:relative;">
            <button
                type="button"
                style="font-size:0.72rem; color:var(--color-text-muted); background:none; border:none; \
                       cursor:pointer; padding:0.15rem 0; text-decoration:underline dotted; \
                       opacity:0.75;"
                onmouseover="this.style.opacity='1'"
                onmouseout="this.style.opacity='0.75'"
                on:click=move |_| set_open.update(|v| *v = !*v)
            >
                {move || if open.get() { "▲ Close icon browser" } else { "▼ Browse icons" }}
            </button>

            <Show when=move || open.get()>
                <div style="margin-top:0.35rem; background:var(--color-bg-primary); \
                             border:1px solid var(--color-border); border-radius:0.5rem; \
                             padding:0.5rem; z-index:10; position:relative;">
                    <input
                        type="text"
                        class="form-input"
                        placeholder="Search 2,700+ self-hosted service icons…"
                        style="font-size:0.78rem; margin-bottom:0.4rem;"
                        prop:value=move || query.get()
                        on:input=move |ev| set_query.set(event_target_value(&ev))
                    />
                    <div style="display:grid; grid-template-columns:repeat(auto-fill, minmax(2.4rem, 1fr)); \
                                gap:0.2rem; max-height:152px; overflow-y:auto;">
                        {move || {
                            let base = cdn_base.get()
                                .map(|b| b.to_string())
                                .unwrap_or_else(|| DEFAULT_CDN.to_string());
                            let icons = manifest.get()
                                .map(|m| m.to_vec())
                                .unwrap_or_default();
                            let q = query.get().to_lowercase();
                            let filtered: Vec<IconEntry> = if q.is_empty() {
                                icons.into_iter().take(60).collect()
                            } else {
                                icons.into_iter()
                                    .filter(|e| {
                                        e.name.to_lowercase().contains(&q)
                                            || e.reference.contains(&q)
                                    })
                                    .take(60)
                                    .collect()
                            };

                            if filtered.is_empty() && manifest.get().is_some() {
                                view! {
                                    <p style="font-size:0.75rem; color:var(--color-text-muted); \
                                               grid-column:1/-1; padding:0.25rem 0;">
                                        "No icons found"
                                    </p>
                                }.into_any()
                            } else {
                                filtered.into_iter().map(|entry| {
                                    let url = format!("{}/svg/{}.svg", base, entry.reference);
                                    let url_clone = url.clone();
                                    let name = entry.name.clone();
                                    view! {
                                        <button
                                            type="button"
                                            title={name}
                                            style="background:none; border:1px solid transparent; \
                                                   border-radius:0.375rem; padding:0.2rem; cursor:pointer; \
                                                   display:flex; align-items:center; justify-content:center; \
                                                   width:2.4rem; height:2.4rem;"
                                            onmouseover="this.style.borderColor='var(--color-accent)'; \
                                                         this.style.background='var(--color-bg-surface)'"
                                            onmouseout="this.style.borderColor='transparent'; \
                                                        this.style.background='none'"
                                            on:click=move |_| {
                                                on_select.run(url_clone.clone());
                                                set_open.set(false);
                                                set_query.set(String::new());
                                            }
                                        >
                                            <img
                                                src={url}
                                                alt=""
                                                style="width:1.5rem; height:1.5rem; object-fit:contain;"
                                                on:error=move |ev| {
                                                    if let Some(t) = ev.target() {
                                                        if let Ok(el) = t.dyn_into::<web_sys::HtmlElement>() {
                                                            let _ = el.style().set_property("display", "none");
                                                        }
                                                    }
                                                }
                                            />
                                        </button>
                                    }.into_any()
                                }).collect_view().into_any()
                            }
                        }}
                    </div>
                </div>
            </Show>
        </div>
    }
}
