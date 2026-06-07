use std::time::Duration;

use sqlx::SqlitePool;
use zbus::zvariant;
use zbus::Connection;

use crate::config::DiscoveryConfig;
use crate::discovery::{DiscoveredUnit, DiscoveryList};

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait Manager {
    fn list_units(&self) -> zbus::Result<Vec<UnitInfo>>;
}

#[derive(Debug, zvariant::Type, serde::Deserialize)]
#[allow(dead_code)]
struct UnitInfo {
    pub name: String,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub followed: String,
    pub object_path: zvariant::OwnedObjectPath,
    pub queued_job_id: u32,
    pub job_type: String,
    pub job_object_path: zvariant::OwnedObjectPath,
}

/// D-Bus proxy for per-unit service properties.
#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Service",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait ServiceUnit {
    #[zbus(property)]
    fn sockets(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    #[zbus(property)]
    fn main_pid(&self) -> zbus::Result<u32>;
}

/// D-Bus proxy for per-unit socket properties.
#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Socket",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SocketUnit {
    #[zbus(property)]
    fn listen(&self) -> zbus::Result<Vec<(String, String)>>;
}

/// Background loop that periodically discovers systemd services via D-Bus.
#[tracing::instrument(skip_all)]
pub async fn discovery_loop(discoveries: DiscoveryList, db: SqlitePool, config: DiscoveryConfig) {
    if !config.enabled {
        tracing::info!("systemd discovery is disabled");
        return;
    }

    let interval = Duration::from_secs(config.interval_secs);
    tracing::info!(?interval, "Starting systemd discovery loop");

    loop {
        if let Err(e) = discover_units(&discoveries, &db, &config).await {
            tracing::error!("Discovery error: {e}");
        }
        tokio::time::sleep(interval).await;
    }
}

/// Perform a single discovery pass: query systemd over D-Bus, filter results,
/// and update the unclaimed discoveries list.
pub async fn discover_units(
    discoveries: &DiscoveryList,
    db: &SqlitePool,
    config: &DiscoveryConfig,
) -> anyhow::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ManagerProxy::new(&connection).await?;

    // ListUnits returns Vec of unit structs
    let units = proxy.list_units().await?;

    let mut unclaimed = Vec::new();

    for unit in &units {
        let name = &unit.name;
        let desc = &unit.description;
        let load_state = &unit.load_state;
        let active_state = &unit.active_state;
        let sub_state = &unit.sub_state;

        // Only .service units that are loaded and active
        if !name.ends_with(".service") {
            continue;
        }
        if load_state != "loaded" || active_state != "active" {
            continue;
        }

        // When server_services_only is enabled, require the unit to be in the
        // "running" sub-state. This drops one-shot OS boot services (e.g.
        // kmod-static-nodes, flatpak-configure-overrides, plymouth-*) that are
        // technically "active" but have already exited.
        if config.server_services_only && sub_state != "running" {
            continue;
        }

        // Check exclusion patterns
        if is_excluded(name, &config.exclude_units) {
            continue;
        }

        // Check if already claimed in DB
        let claimed = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM services WHERE systemd_unit = ? LIMIT 1)",
        )
        .bind(name)
        .fetch_one(db)
        .await
        .unwrap_or(false);

        if claimed {
            continue;
        }

        // Attempt to detect the TCP port this service is listening on.
        let url_hint = detect_url_hint(&connection, &unit.object_path).await;

        unclaimed.push(DiscoveredUnit {
            unit_name: name.clone(),
            description: desc.clone(),
            active_state: active_state.clone(),
            sub_state: sub_state.clone(),
            source: "systemd".to_string(),
            url_hint,
        });
    }

    // Replace only systemd entries, preserving container discoveries
    let mut list = discoveries.write().await;
    list.retain(|u| u.source != "systemd");
    list.extend(unclaimed);
    tracing::debug!(count = list.len(), "systemd discovery pass complete");

    Ok(())
}

/// Try to build a `http://localhost:{port}` URL hint for a service unit.
///
/// Attempts two strategies in order:
/// 1. Socket-activation: reads the unit's `Sockets` property and then each
///    socket's `Listen` property to find a bound TCP port.
/// 2. Procfs: reads `MainPID` and then `/proc/{pid}/net/tcp[6]` to find the
///    first TCP port in LISTEN state.
///
/// Returns `None` if neither strategy finds a port (e.g. Unix-socket-only
/// service, or D-Bus / filesystem permission denied).
async fn detect_url_hint(
    connection: &Connection,
    object_path: &zvariant::OwnedObjectPath,
) -> Option<String> {
    if let Some(port) = detect_port_via_sockets(connection, object_path).await {
        return Some(format!("http://localhost:{port}"));
    }
    if let Some(port) = detect_port_via_proc(connection, object_path).await {
        return Some(format!("http://localhost:{port}"));
    }
    None
}

/// Stage 1: query D-Bus `Sockets` → per-socket `Listen` → first TCP port.
async fn detect_port_via_sockets(
    connection: &Connection,
    object_path: &zvariant::OwnedObjectPath,
) -> Option<u16> {
    let service_proxy = ServiceUnitProxy::builder(connection)
        .path(object_path.as_str())
        .ok()?
        .build()
        .await
        .ok()?;

    let socket_paths = service_proxy.sockets().await.ok()?;

    for sock_path in &socket_paths {
        let socket_proxy = SocketUnitProxy::builder(connection)
            .path(sock_path.as_str())
            .ok()?
            .build()
            .await
            .ok()?;

        let entries = socket_proxy.listen().await.ok()?;
        for (_kind, address) in &entries {
            if let Some(port) = parse_port_from_listen_address(address) {
                return Some(port);
            }
        }
    }

    None
}

/// Stage 2: read `MainPID` from D-Bus, then scan `/proc/{pid}/net/tcp[6]`.
async fn detect_port_via_proc(
    connection: &Connection,
    object_path: &zvariant::OwnedObjectPath,
) -> Option<u16> {
    let service_proxy = ServiceUnitProxy::builder(connection)
        .path(object_path.as_str())
        .ok()?
        .build()
        .await
        .ok()?;

    let pid = service_proxy.main_pid().await.ok()?;
    if pid == 0 {
        return None;
    }

    parse_tcp_listen_port(pid).await
}

/// Scan `/proc/{pid}/net/tcp` then `/proc/{pid}/net/tcp6` for the first port
/// in LISTEN state (state field == `"0A"`).
async fn parse_tcp_listen_port(pid: u32) -> Option<u16> {
    for filename in &["tcp", "tcp6"] {
        let path = format!("/proc/{pid}/net/{filename}");
        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        // Skip the header line
        for line in content.lines().skip(1) {
            let mut cols = line.split_ascii_whitespace();
            let _sl = cols.next()?;
            let local_addr = cols.next()?;
            let _rem_addr = cols.next()?;
            let state = cols.next()?;

            if state != "0A" {
                // Not TCP_LISTEN
                continue;
            }

            // local_addr is "{hex_ip}:{hex_port}" — port is after the last ':'
            if let Some(hex_port) = local_addr.rsplit(':').next() {
                if let Ok(port) = u16::from_str_radix(hex_port, 16) {
                    if port > 0 {
                        return Some(port);
                    }
                }
            }
        }
    }
    None
}

/// Parse a TCP port number from a systemd socket `Listen` address string.
///
/// Handles:
/// - `"0.0.0.0:8080"` / `"[::]:8080"` / `"127.0.0.1:8080"` → 8080
/// - `"8080"` (port only) → 8080
/// - `"/run/app.sock"` (Unix socket) → None
/// - `""` (empty) → None
fn parse_port_from_listen_address(address: &str) -> Option<u16> {
    let address = address.trim();
    if address.is_empty() || address.starts_with('/') {
        return None;
    }
    // Try splitting on last ':' to extract port from "host:port" form
    if let Some(hex_or_dec) = address.rsplit(':').next() {
        if let Ok(port) = hex_or_dec.parse::<u16>() {
            if port > 0 {
                return Some(port);
            }
        }
    }
    // Fall back: entire string might be just a port number
    address.parse::<u16>().ok().filter(|&p| p > 0)
}

/// Check if a unit name matches any exclusion pattern (supports `*` glob, including mid-pattern).
fn is_excluded(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if let Some(star_pos) = pattern.find('*') {
            let prefix = &pattern[..star_pos];
            let suffix = &pattern[star_pos + 1..];
            if name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
            {
                return true;
            }
        } else if name == pattern {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exclusion_exact() {
        let patterns = vec!["dbus.service".to_string()];
        assert!(is_excluded("dbus.service", &patterns));
        assert!(!is_excluded("nginx.service", &patterns));
    }

    #[test]
    fn test_exclusion_glob() {
        let patterns = vec!["systemd-*.service".to_string()];
        assert!(is_excluded("systemd-journald.service", &patterns));
        assert!(is_excluded("systemd-logind.service", &patterns));
        assert!(!is_excluded("nginx.service", &patterns));
    }

    #[test]
    fn test_parse_port_host_colon_port() {
        assert_eq!(parse_port_from_listen_address("0.0.0.0:8080"), Some(8080));
        assert_eq!(parse_port_from_listen_address("127.0.0.1:3000"), Some(3000));
        assert_eq!(parse_port_from_listen_address("[::]:443"), Some(443));
    }

    #[test]
    fn test_parse_port_port_only() {
        assert_eq!(parse_port_from_listen_address("8080"), Some(8080));
        assert_eq!(parse_port_from_listen_address("80"), Some(80));
    }

    #[test]
    fn test_parse_port_unix_socket_excluded() {
        assert_eq!(parse_port_from_listen_address("/run/app.sock"), None);
        assert_eq!(parse_port_from_listen_address("/tmp/foo.socket"), None);
    }

    #[test]
    fn test_parse_port_empty() {
        assert_eq!(parse_port_from_listen_address(""), None);
        assert_eq!(parse_port_from_listen_address("   "), None);
    }

    #[test]
    fn test_parse_port_zero_excluded() {
        assert_eq!(parse_port_from_listen_address("0.0.0.0:0"), None);
        assert_eq!(parse_port_from_listen_address("0"), None);
    }
}
