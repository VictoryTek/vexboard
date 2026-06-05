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
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub assets_path: String,
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
    pub max_history: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub push_interval_ms: u64,
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
                    .separator("__")
                    .try_parsing(true),
            );

        let cfg = builder.build()?;
        let app_config: AppConfig = cfg.try_deserialize()?;
        Ok(app_config)
    }
}
