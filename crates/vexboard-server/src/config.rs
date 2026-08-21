use serde::Deserialize;
use std::path::PathBuf;

/// Top-level application configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub discovery: DiscoveryConfig,
    pub docker: DockerConfig,
    pub probe: ProbeConfig,
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub assets_path: String,
    /// Origins permitted by CORS. Use `["*"]` to allow any origin (default).
    /// In production set this to your frontend URL, e.g. `["https://dashboard.example.com"]`.
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    /// Base URL for the selfhst/icons CDN used by the icon picker in the UI.
    /// Override with your own selfhst/icons Docker instance URL for air-gapped deployments.
    #[serde(default = "default_icon_cdn_base")]
    pub icon_cdn_base: String,
}

fn default_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_icon_cdn_base() -> String {
    "https://cdn.jsdelivr.net/gh/selfhst/icons@main".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub secret: String,
    pub session_ttl_hours: u64,
    /// Set to true when the server is behind TLS (enables the Secure cookie flag).
    /// Leave false for plain-HTTP self-hosted deployments on a local network.
    #[serde(default)]
    pub secure_cookies: bool,
    /// Maximum login attempts per IP address within the rate-limit window before
    /// returning 429 Too Many Requests. Set to 0 to disable rate limiting.
    #[serde(default = "default_login_rate_limit_attempts")]
    pub login_rate_limit_attempts: u32,
    /// Sliding window duration in seconds for the login rate limiter.
    #[serde(default = "default_login_rate_limit_window_secs")]
    pub login_rate_limit_window_secs: u64,
    /// Authentication mode: "session" (default, login required) or "none"
    /// (all API routes open — only safe when the network layer itself
    /// restricts access, e.g. Tailscale-only or an isolated LAN).
    #[serde(default = "default_auth_mode")]
    pub mode: String,
    /// Set to true when the server sits behind a reverse proxy that sets
    /// X-Forwarded-For. When false (default), the header is ignored entirely and
    /// the real socket address is always used — client-supplied X-Forwarded-For
    /// values are otherwise fully spoofable and would defeat the login rate limiter.
    #[serde(default)]
    pub behind_proxy: bool,
    /// OS usernames that receive the admin role when authenticating via PAM.
    /// All other successfully PAM-authenticated users get the viewer role.
    /// Only read when the `pam-auth` feature is compiled in.
    #[serde(default)]
    pub pam_admin_users: Vec<String>,
}

fn default_login_rate_limit_attempts() -> u32 {
    10
}

fn default_login_rate_limit_window_secs() -> u64 {
    60
}

fn default_auth_mode() -> String {
    "session".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub exclude_units: Vec<String>,
    /// When true, only show services whose unit file lives under /etc/systemd/system/
    /// (i.e. explicitly installed/enabled by an admin), filtering out OS-managed
    /// services from /lib/systemd/system/ or /usr/lib/systemd/system/.
    #[serde(default = "default_true")]
    pub server_services_only: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct DockerConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    /// Unix socket paths to try in order (Docker then Podman)
    pub sockets: Vec<String>,
    pub exclude_images: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeConfig {
    pub default_interval_secs: u64,
    pub timeout_secs: u64,
    /// How many days of probe_results to keep per service before pruning.
    pub history_retention_days: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub push_interval_ms: u64,
}

/// A single webhook endpoint configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    /// Event types to deliver. Empty means all events are delivered.
    /// Supported values: `"service.down"`, `"service.up"`
    #[serde(default)]
    pub events: Vec<String>,
    /// Per-webhook HMAC-SHA256 signing secret. Overrides the global `webhook_secret` when set.
    #[serde(default)]
    pub secret: String,
}

/// Notification / webhook configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct NotificationsConfig {
    /// Global HMAC-SHA256 signing secret applied to all webhooks that do not set their own
    /// `secret`. Leave empty to disable request signing.
    #[serde(default)]
    pub webhook_secret: String,
    /// Number of retry attempts after an initial delivery failure (default 2).
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// Base delay in seconds between retries, multiplied by the attempt number (default 2).
    #[serde(default = "default_retry_delay_secs")]
    pub retry_delay_secs: u64,
    /// Webhook endpoint configurations.
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
}

fn default_retry_count() -> u32 {
    2
}

fn default_retry_delay_secs() -> u64 {
    2
}

impl AppConfig {
    /// Load configuration from file and environment variables.
    ///
    /// Priority (highest to lowest):
    /// 1. Environment variables prefixed with `VEXBOARD_` (separator: `__`)
    /// 2. `/etc/vexboard/config.toml` (if exists)
    /// 3. `config/default.toml` (bundled defaults)
    pub fn load() -> anyhow::Result<Self> {
        let builder = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name("/etc/vexboard/config").required(false))
            .add_source(
                config::Environment::with_prefix("VEXBOARD")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            );

        let cfg = builder.build()?;
        let app_config: AppConfig = cfg.try_deserialize()?;
        match app_config.auth.mode.as_str() {
            "session" | "none" => {}
            other => anyhow::bail!(
                "invalid auth.mode {:?}: expected \"session\" or \"none\"",
                other
            ),
        }
        if app_config.auth.mode == "session" && app_config.auth.secret.len() < 32 {
            anyhow::bail!(
                "auth.secret must be at least 32 bytes (got {}); generate one with `openssl rand -base64 48`",
                app_config.auth.secret.len()
            );
        }
        Ok(app_config)
    }
}
