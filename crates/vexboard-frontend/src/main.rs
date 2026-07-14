mod components;
mod pages;

use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use leptos::task::spawn_local;
use leptos_router::components::{Outlet, ParentRoute, Route, Router, Routes};
use leptos_router::path;

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub username: String,
    pub role: String,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

fn main() {
    console_error_panic_hook::set_once();

    #[cfg(target_arch = "wasm32")]
    if let Some(loader) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("initial-loader"))
    {
        loader.remove();
    }

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

    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Page not found"</p> }>
                // Full-screen bare routes — no sidebar or metric bar
                <Route path=path!("/setup") view=pages::setup::SetupPage />
                <Route path=path!("/login") view=pages::login::LoginPage />
                // Main app: sidebar + metric bar wrapping child routes
                <ParentRoute path=path!("/") view=MainLayout>
                    <Route path=path!("") view=pages::dashboard::DashboardPage />
                    <Route path=path!("discovered") view=pages::discovered::DiscoveredPage />
                    <Route path=path!("settings") view=pages::settings::SettingsPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

#[component]
fn MainLayout() -> impl IntoView {
    // Fetch current user (username + role) and provide via context.
    // Defaults to viewer until resolved so write UI is hidden while loading.
    let current_user: RwSignal<Option<CurrentUser>> = RwSignal::new(None);
    provide_context(current_user);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(resp) = gloo_net::http::Request::get("/api/v1/auth/me").send().await {
                if resp.status() == 401 {
                    // Avoid a race: check setup status before deciding where to redirect.
                    // Without this, the auth 401 could send the user to /login before the
                    // first-run setup has been completed.
                    let needs_setup = async {
                        let r = gloo_net::http::Request::get("/api/v1/setup/status")
                            .send()
                            .await
                            .ok()?;
                        let body = r.json::<serde_json::Value>().await.ok()?;
                        body["needs_setup"].as_bool()
                    }
                    .await
                    .unwrap_or(false);
                    web_sys::window()
                        .unwrap()
                        .location()
                        .set_href(if needs_setup { "/setup" } else { "/login" })
                        .ok();
                    return;
                }
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let username = body["user"]["username"].as_str().unwrap_or("").to_string();
                    let role = body["user"]["role"]
                        .as_str()
                        .unwrap_or("viewer")
                        .to_string();
                    current_user.set(Some(CurrentUser { username, role }));
                }
            }
        });
    });

    view! {
        <div class="app-shell">
            <components::sidebar::Sidebar />
            <main class="app-main">
                <components::metric_bar::MetricBar />
                <div class="app-content">
                    <Outlet />
                </div>
            </main>
        </div>
    }
}
