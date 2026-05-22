mod components;
mod pages;

use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> });
}

#[component]
fn App() -> impl IntoView {
    let initial_mode = {
        #[cfg(target_arch = "wasm32")]
        {
            components::sidebar::load_sidebar_mode_from_storage()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            components::sidebar::SidebarMode::HoverExpand
        }
    };
    let (sidebar_mode, set_sidebar_mode) = signal(initial_mode);
    provide_context(sidebar_mode);
    provide_context(set_sidebar_mode);

    // First-run guard: redirect to /setup if no users exist yet
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        spawn_local(async move {
            let current_path = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_default();
            if current_path == "/setup" || current_path == "/login" {
                return;
            }
            if let Ok(resp) = gloo_net::http::Request::get("/api/v1/setup/status")
                .send()
                .await
            {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if body["needs_setup"].as_bool().unwrap_or(false) {
                        web_sys::window()
                            .unwrap()
                            .location()
                            .set_href("/setup")
                            .ok();
                    }
                }
            }
        });
    });

    view! {
        <Router>
            <div class="flex h-screen overflow-hidden">
                <components::sidebar::Sidebar />
                <main class="flex-1 flex flex-col overflow-hidden">
                    <components::metric_bar::MetricBar />
                    <div class="flex-1 overflow-auto p-6">
                        <Routes fallback=|| view! { <p>"Page not found"</p> }>
                            <Route path=path!("/") view=pages::dashboard::DashboardPage />
                            <Route path=path!("/settings") view=pages::settings::SettingsPage />
                            <Route path=path!("/login") view=pages::login::LoginPage />
                            <Route path=path!("/setup") view=pages::setup::SetupPage />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}
