use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Debug, Clone, serde::Deserialize)]
struct ImportSummary {
    groups_created: i64,
    groups_reused: i64,
    services_created: i64,
    services_skipped: i64,
    quick_links_created: i64,
    notification_channels_created: i64,
    notification_channels_skipped: i64,
}

async fn fetch_nix_snippet() -> Option<String> {
    let resp = gloo_net::http::Request::get("/api/v1/config/export/nix")
        .send()
        .await
        .ok()?;
    if !resp.ok() {
        return None;
    }
    resp.text().await.ok()
}

#[cfg(target_arch = "wasm32")]
async fn send_import(body: String) -> Result<ImportSummary, String> {
    let req = gloo_net::http::Request::post("/api/v1/config/import")
        .header("content-type", "application/json")
        .body(body)
        .map_err(|_| "Failed to build the import request.".to_string())?;
    let resp = req
        .send()
        .await
        .map_err(|_| "Could not reach the server.".to_string())?;
    if !resp.ok() {
        let msg = resp
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "Import failed.".to_string());
        return Err(msg);
    }
    resp.json::<ImportSummary>()
        .await
        .map_err(|_| "Import succeeded, but the response couldn't be read.".to_string())
}

fn summarize(s: &ImportSummary) -> String {
    format!(
        "Added {} group(s) ({} already existed), {} service(s) ({} skipped — already claimed), \
         {} quick link(s), {} notification destination(s) ({} skipped).",
        s.groups_created,
        s.groups_reused,
        s.services_created,
        s.services_skipped,
        s.quick_links_created,
        s.notification_channels_created,
        s.notification_channels_skipped,
    )
}

#[component]
pub(super) fn BackupSection() -> impl IntoView {
    let nix_snippet: RwSignal<Option<String>> = RwSignal::new(None);
    let import_result: RwSignal<Option<Result<ImportSummary, String>>> = RwSignal::new(None);
    let importing = RwSignal::new(false);
    let file_input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        spawn_local(async move {
            nix_snippet.set(fetch_nix_snippet().await);
        });
    });

    let on_file_change = move |_ev: leptos::ev::Event| {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::closure::Closure;
            use wasm_bindgen::JsCast;
            use web_sys::FileReader;

            let Some(input) = file_input_ref.get_untracked() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            let Some(file) = files.get(0) else {
                return;
            };

            if let Ok(reader) = FileReader::new() {
                let reader_for_closure = reader.clone();
                let onload = Closure::wrap(Box::new(move |_evt: web_sys::Event| {
                    if let Ok(result) = reader_for_closure.result() {
                        if let Some(text) = result.as_string() {
                            importing.set(true);
                            import_result.set(None);
                            spawn_local(async move {
                                let outcome = send_import(text).await;
                                importing.set(false);
                                import_result.set(Some(outcome));
                            });
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                onload.forget();
                let _ = reader.read_as_text(&file);
            }
        }
    };

    view! {
        <div>
            <p class="settings-pane-title">"Backup & Data"</p>
            <p class="settings-pane-sub">"Your groups, services, quick links and notification destinations as one portable file."</p>

            <div class="settings-card">
                <div class="settings-card-head">"Export"</div>
                <div class="settings-card-row">
                    <div class="settings-card-row-txt">
                        <p class="settings-card-row-label">"Download a backup"</p>
                        <p class="settings-card-row-hint">
                            "A JSON file with your groups, services, quick links and notification \
                             destinations. Passwords and notification signing secrets are never included."
                        </p>
                    </div>
                    <div class="settings-card-row-ctl">
                        <a class="btn-primary" href="/api/v1/config/export" download="vexboard-config.json">"Download"</a>
                    </div>
                </div>
            </div>

            <div class="settings-card">
                <div class="settings-card-head">"Restore"</div>
                <div class="settings-card-row">
                    <div class="settings-card-row-txt">
                        <p class="settings-card-row-label">"Restore from a backup"</p>
                        <p class="settings-card-row-hint">
                            "Adds groups, services, quick links and destinations from the file. \
                             Nothing already here is changed or removed — a group with a matching \
                             name is reused, and a service whose unit is already claimed is skipped."
                        </p>
                    </div>
                    <div class="settings-card-row-ctl">
                        <input
                            type="file"
                            accept="application/json"
                            node_ref=file_input_ref
                            on:change=on_file_change
                            disabled=move || importing.get()
                        />
                    </div>
                </div>
                {move || import_result.get().map(|outcome| match outcome {
                    Ok(s) => view! {
                        <div class="settings-card-row">
                            <p class="settings-form-success">{summarize(&s)}</p>
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="settings-card-row">
                            <p class="settings-form-error">{e}</p>
                        </div>
                    }.into_any(),
                })}
            </div>

            <div class="settings-card">
                <div class="settings-card-head">"Nix Configuration"</div>
                <div class="settings-card-row settings-card-row-stack">
                    <div class="settings-card-row-txt">
                        <p class="settings-card-row-label">"Copy as Nix"</p>
                        <p class="settings-card-row-hint">
                            "The discovery, Docker, probe and notification-delivery settings this \
                             instance is currently running with, as a services.vexboard.settings \
                             block — click the box below to select it all. Secrets are never \
                             included; use secretFile for those, per the README."
                        </p>
                    </div>
                    <textarea
                        readonly=true
                        class="form-input"
                        style="font-family:ui-monospace,monospace; font-size:0.75rem; min-height:220px; resize:vertical;"
                        prop:value=move || nix_snippet.get().unwrap_or_else(|| "Loading…".to_string())
                        on:click=move |ev| {
                            use wasm_bindgen::JsCast;
                            if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlTextAreaElement>().ok()) {
                                el.select();
                            }
                        }
                    ></textarea>
                </div>
            </div>
        </div>
    }
}
