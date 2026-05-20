mod components;
mod pages;

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> });
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <div class="flex h-screen overflow-hidden">
                <components::sidebar::Sidebar />
                <main class="flex-1 overflow-y-auto">
                    <components::metric_bar::MetricBar />
                    <div class="p-6">
                        <Routes fallback=|| view! { <p>"Page not found"</p> }>
                            <Route path=path!("/") view=pages::dashboard::DashboardPage />
                            <Route path=path!("/settings") view=pages::settings::SettingsPage />
                            <Route path=path!("/login") view=pages::login::LoginPage />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}
