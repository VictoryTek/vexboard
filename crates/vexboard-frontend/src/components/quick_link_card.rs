use leptos::prelude::*;

#[derive(Debug, Clone)]
pub struct QuickLinkData {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
}

#[component]
pub fn QuickLinkCard(
    link: QuickLinkData,
    #[prop(into)] on_delete: Callback<i64>,
    #[prop(into)] on_edit: Callback<i64>,
) -> impl IntoView {
    let link_id = link.id;

    let first = link.title.chars().next().unwrap_or('?');
    let letter = first.to_ascii_uppercase().to_string();
    let icon_opt = link.icon.clone().filter(|i| !i.is_empty());
    let is_url_icon = icon_opt
        .as_ref()
        .is_some_and(|i| i.starts_with("http://") || i.starts_with("https://"));
    let icon_text = if is_url_icon {
        letter.clone()
    } else {
        icon_opt.clone().unwrap_or(letter)
    };
    let icon_url = if is_url_icon { icon_opt } else { None };

    let url = link.url.clone();
    let description = link.description.clone().filter(|d| !d.trim().is_empty());

    view! {
        <a
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            class="service-card"
            style="display:block; text-decoration:none; cursor:pointer;"
            onmouseover="this.style.borderColor='var(--color-accent)'"
            onmouseout="this.style.borderColor=''"
        >
            <div style="display:flex; align-items:center; gap:0.75rem; min-width:0;">
                <div class="service-icon" style="position:relative; flex-shrink:0;">
                    <span>{icon_text}</span>
                    {icon_url.map(|src| view! {
                        <img src={src} alt=""
                            style="position:absolute;top:0;left:0;width:100%;height:100%;object-fit:contain;border-radius:inherit;padding:3px;"
                            on:error=move |ev| {
                                use wasm_bindgen::JsCast;
                                if let Some(t) = ev.target() {
                                    if let Ok(el) = t.dyn_into::<web_sys::HtmlElement>() {
                                        let _ = el.style().set_property("display", "none");
                                    }
                                }
                            }
                        />
                    })}
                </div>
                <div style="min-width:0; flex:1;">
                    <p style="font-size:0.9rem; font-weight:600; color:var(--color-text-primary); \
                               margin:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                        {link.title}
                    </p>
                    {description.map(|d| view! {
                        <p style="font-size:0.75rem; color:var(--color-text-secondary); \
                                   margin:0.1rem 0 0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">
                            {d}
                        </p>
                    })}
                </div>
            </div>

            // Edit / Delete actions — stop propagation so clicks don't follow the link
            <div style="display:flex; justify-content:flex-end; gap:0.75rem; margin-top:0.5rem;"
                on:click=move |ev| { ev.prevent_default(); ev.stop_propagation(); }
            >
                <button
                    style="background:none; border:none; cursor:pointer; \
                           color:var(--color-text-muted); opacity:0.35; padding:0.15rem 0; \
                           font-size:0.7rem; display:flex; align-items:center; gap:0.25rem; line-height:1;"
                    onmouseover="this.style.opacity='1'; this.style.color='var(--color-accent)'"
                    onmouseout="this.style.opacity='0.35'; this.style.color='var(--color-text-muted)'"
                    on:click=move |_| on_edit.run(link_id)
                >
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2"
                         stroke-linecap="round" stroke-linejoin="round">
                        <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                        <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                    </svg>
                    "Edit"
                </button>
                <button
                    style="background:none; border:none; cursor:pointer; \
                           color:var(--color-text-muted); opacity:0.35; padding:0.15rem 0; \
                           font-size:0.7rem; display:flex; align-items:center; gap:0.25rem; line-height:1;"
                    onmouseover="this.style.opacity='1'; this.style.color='var(--color-danger)'"
                    onmouseout="this.style.opacity='0.35'; this.style.color='var(--color-text-muted)'"
                    on:click=move |_| on_delete.run(link_id)
                >
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2"
                         stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="3 6 5 6 21 6"/>
                        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                    </svg>
                    "Remove"
                </button>
            </div>
        </a>
    }
}
