mod components;
mod pages;

use leptos::*;
use leptos_router::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
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
                        <Routes>
                            <Route path="/" view=pages::dashboard::DashboardPage />
                            <Route path="/settings" view=pages::settings::SettingsPage />
                            <Route path="/login" view=pages::login::LoginPage />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}
