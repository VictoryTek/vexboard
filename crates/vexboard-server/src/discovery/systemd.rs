use std::collections::HashSet;
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

/// D-Bus proxy for per-unit service properties (org.freedesktop.systemd1.Service).
#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Service",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait ServiceUnit {
    #[zbus(property)]
    fn sockets(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    // Explicit name: zbus converts fn snake_case → CamelCase ("MainPid") but
    // the actual D-Bus property is "MainPID" (all-caps acronym).
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> zbus::Result<u32>;
}

/// D-Bus proxy for per-unit socket properties (org.freedesktop.systemd1.Socket).
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
        let url_hint = detect_url_hint(&connection, &unit.object_path, name).await;
        tracing::info!(unit = %name, url_hint = ?url_hint, "url hint detection result");

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

/// Outcome of OCI container runtime detection.
enum OciDetect {
    /// OCI service and a host port was discovered.
    Found(u16),
    /// OCI service confirmed but no port could be discovered.
    /// Callers should return `None` rather than falling through to UID matching,
    /// which would produce false positives (e.g. CUPS on port 631 also running as root).
    NoPort,
    /// The service's main process is not a container runtime; continue normal detection.
    NotOci,
}

/// Try to build a `http://localhost:{port}` URL hint for a service unit.
///
/// Four detection strategies in priority order:
/// 1. Socket-activation: reads the unit's Sockets D-Bus property, then each
///    socket's Listen property for a bound TCP port.
/// 2. OCI detection: reads /proc/{MainPID}/exe; if it is a container runtime
///    (podman, docker) queries `podman port` / `docker port` for host bindings.
///    If OCI is confirmed but no port is found, returns None without falling
///    through to UID matching (prevents CUPS-port false positives).
/// 3. MainPID + inode match: reads the service's MainPID, collects its open
///    socket inodes from /proc/{pid}/fd/, matches against /proc/{pid}/net/tcp[6].
/// 4. cgroup.procs: iterates all PIDs in the service cgroup and applies the same
///    inode-matched scan as stage 3.
async fn detect_url_hint(
    connection: &Connection,
    object_path: &zvariant::OwnedObjectPath,
    unit: &str,
) -> Option<String> {
    let path = object_path.as_str();

    // Stage 1 — socket activation
    if let Some(port) = detect_via_socket_activation(connection, path, unit).await {
        return Some(format!("http://localhost:{port}"));
    }

    // Stage 2 — OCI container runtime detection
    match detect_via_oci(connection, path, unit).await {
        OciDetect::Found(port) => return Some(format!("http://localhost:{port}")),
        // OCI confirmed but no port — skip UID-matching stages to prevent false positives
        OciDetect::NoPort => return None,
        OciDetect::NotOci => {}
    }

    // Stage 3 — MainPID + inode-matched procfs (non-OCI only)
    if let Some(port) = detect_via_main_pid(connection, path, unit).await {
        return Some(format!("http://localhost:{port}"));
    }

    // Stage 4 — cgroup.procs fallback (non-OCI only)
    if let Some(port) = detect_via_cgroup(unit).await {
        return Some(format!("http://localhost:{port}"));
    }

    None
}

/// Stage 2 — detect OCI container services and query port bindings from the runtime.
///
/// Reads `MainPID` via D-Bus, checks `/proc/{pid}/exe` for a container runtime
/// binary, then runs `podman port` or `docker port` to obtain host-side TCP port
/// bindings for the container.
async fn detect_via_oci(connection: &Connection, path: &str, unit: &str) -> OciDetect {
    // Read MainPID
    let pid = match read_main_pid(connection, path, unit).await {
        Some(p) if p > 0 => p,
        _ => return OciDetect::NotOci,
    };

    // Check the executable of the main process
    let exe_path = match tokio::fs::read_link(format!("/proc/{pid}/exe")).await {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(unit, pid, error = %e, "oci: could not read /proc/{pid}/exe");
            return OciDetect::NotOci;
        }
    };

    let runtime_bin: &str = match exe_path.file_name().and_then(|n| n.to_str()) {
        Some("podman") | Some("podman-remote") => "podman",
        Some("docker") => "docker",
        other => {
            tracing::debug!(unit, pid, exe = ?other, "oci: main process is not a container runtime");
            return OciDetect::NotOci;
        }
    };

    tracing::debug!(
        unit,
        pid,
        runtime = runtime_bin,
        "oci: detected container runtime"
    );

    // Derive container name candidates from the unit name
    let base = unit.strip_suffix(".service").unwrap_or(unit);
    let candidates = [base.to_string(), format!("systemd-{base}")];

    for name in &candidates {
        if let Some(port) = query_container_port(runtime_bin, name, unit).await {
            tracing::info!(unit, container = %name, port, "oci: discovered container port");
            return OciDetect::Found(port);
        }
    }

    tracing::debug!(
        unit,
        "oci: runtime detected but no port found for any candidate name"
    );
    OciDetect::NoPort
}

/// Run `<runtime> port <container_name>` and return the first valid host port.
async fn query_container_port(runtime: &str, container: &str, unit: &str) -> Option<u16> {
    let output = match tokio::process::Command::new(runtime)
        .args(["port", container])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::debug!(unit, runtime, container, error = %e, "oci: failed to spawn runtime");
            return None;
        }
    };

    if !output.status.success() {
        tracing::debug!(
            unit,
            runtime,
            container,
            status = %output.status,
            "oci: port query returned non-zero (container may not exist or not be running)"
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    tracing::debug!(unit, container, output = %stdout.trim(), "oci: port output");
    parse_docker_port_output(&stdout)
}

/// Parse the first valid host port from `docker port` / `podman port` output.
///
/// Each line has the form:
///   `{container-port}/tcp -> {host-ip}:{host-port}`
/// e.g. `8444/tcp -> 0.0.0.0:8444` or `80/tcp -> :::80`
fn parse_docker_port_output(output: &str) -> Option<u16> {
    for line in output.lines() {
        let Some(arrow) = line.find("->") else {
            continue;
        };
        let host_part = line[arrow + 2..].trim();
        if let Some(port_str) = host_part.rsplit(':').next() {
            if let Ok(port) = port_str.trim().parse::<u16>() {
                if port > 0 {
                    return Some(port);
                }
            }
        }
    }
    None
}

/// Read the `MainPID` property from the service unit's D-Bus object.
async fn read_main_pid(connection: &Connection, path: &str, unit: &str) -> Option<u32> {
    let builder = match ServiceUnitProxy::builder(connection).path(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(unit, error = %e, "read_main_pid: invalid object path");
            return None;
        }
    };
    match builder.build().await {
        Ok(proxy) => match proxy.main_pid().await {
            Ok(pid) => Some(pid),
            Err(e) => {
                tracing::warn!(unit, error = %e, "read_main_pid: failed to read MainPID");
                None
            }
        },
        Err(e) => {
            tracing::warn!(unit, error = %e, "read_main_pid: failed to build proxy");
            None
        }
    }
}

/// Stage 1 — query D-Bus Sockets → per-socket Listen → first TCP port.
async fn detect_via_socket_activation(
    connection: &Connection,
    path: &str,
    unit: &str,
) -> Option<u16> {
    let builder = match ServiceUnitProxy::builder(connection).path(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(unit, error = %e, "stage1: invalid unit object path");
            return None;
        }
    };
    let service_proxy = match builder.build().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(unit, error = %e, "stage1: failed to build service proxy");
            return None;
        }
    };

    let socket_paths = match service_proxy.sockets().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(unit, error = %e, "stage1: failed to read Sockets property");
            return None;
        }
    };

    if socket_paths.is_empty() {
        tracing::debug!(
            unit,
            "stage1: no associated socket units (no socket activation)"
        );
        return None;
    }

    tracing::debug!(
        unit,
        count = socket_paths.len(),
        "stage1: found socket units"
    );

    for sock_path in &socket_paths {
        let Ok(builder) = SocketUnitProxy::builder(connection).path(sock_path.as_str()) else {
            tracing::warn!(unit, sock = %sock_path, "stage1: invalid socket object path");
            continue;
        };
        let socket_proxy = match builder.build().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(unit, sock = %sock_path, error = %e, "stage1: failed to build socket proxy");
                continue;
            }
        };
        let entries = match socket_proxy.listen().await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(unit, sock = %sock_path, error = %e, "stage1: failed to read Listen property");
                continue;
            }
        };
        for (kind, address) in &entries {
            tracing::debug!(unit, kind, address, "stage1: listen entry");
            if let Some(port) = parse_port_from_listen_address(address) {
                tracing::info!(unit, port, "stage1: detected port via socket activation");
                return Some(port);
            }
        }
    }

    None
}

/// Stage 2 — read MainPID via D-Bus, then scan procfs with inode matching.
async fn detect_via_main_pid(connection: &Connection, path: &str, unit: &str) -> Option<u16> {
    let service_proxy = match ServiceUnitProxy::builder(connection).path(path) {
        Ok(b) => match b.build().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(unit, error = %e, "stage2: failed to build service proxy");
                return None;
            }
        },
        Err(e) => {
            tracing::warn!(unit, error = %e, "stage2: invalid unit object path");
            return None;
        }
    };

    let pid = match service_proxy.main_pid().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(unit, error = %e, "stage2: failed to read MainPID property");
            return None;
        }
    };

    if pid == 0 {
        tracing::debug!(unit, "stage2: MainPID is 0");
        return None;
    }

    tracing::debug!(unit, pid, "stage2: scanning procfs");
    listen_port_for_pid(pid, unit).await
}

/// Stage 3 — read all PIDs from the service cgroup, scan each with inode matching.
async fn detect_via_cgroup(unit: &str) -> Option<u16> {
    let cgroup_path = format!("/sys/fs/cgroup/system.slice/{unit}/cgroup.procs");
    let content = match tokio::fs::read_to_string(&cgroup_path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(unit, error = %e, "stage3: cgroup.procs not readable");
            return None;
        }
    };
    tracing::debug!(unit, "stage3: scanning cgroup PIDs");
    for line in content.lines() {
        let Ok(pid) = line.trim().parse::<u32>() else {
            continue;
        };
        if let Some(port) = listen_port_for_pid(pid, unit).await {
            return Some(port);
        }
    }
    None
}

/// Find the first TCP LISTEN port owned by `pid` using socket inode matching.
///
/// Ownership is determined in priority order:
/// 1. Inode matching: read /proc/{pid}/fd/ symlinks, match socket:[inode] against
///    the inode column of /proc/{pid}/net/tcp[6]. Most accurate.
/// 2. UID matching: when /proc/{pid}/fd/ is unreadable (different user, no
///    CAP_SYS_PTRACE), read the process effective UID from /proc/{pid}/status and
///    match against the uid column of the TCP table. Avoids false positives from
///    other services (e.g. CUPS on port 631 before the target service's port).
/// 3. Neither readable: return None rather than guess.
async fn listen_port_for_pid(pid: u32, unit: &str) -> Option<u16> {
    let socket_inodes = collect_socket_inodes(pid).await;

    if let Some(ref inodes) = socket_inodes {
        tracing::debug!(
            unit,
            pid,
            count = inodes.len(),
            "collected socket inodes for pid"
        );
        if inodes.is_empty() {
            tracing::debug!(unit, pid, "pid has no open sockets");
            return None;
        }
    }

    // When inode data is unavailable, fall back to UID matching: read the
    // process effective UID from /proc/{pid}/status (always world-readable) and
    // filter the TCP table by the uid column.
    let proc_uid: Option<u32> = if socket_inodes.is_none() {
        let uid = read_proc_uid(pid).await;
        tracing::debug!(unit, pid, uid = ?uid, "fd/ unreadable, falling back to uid matching");
        uid
    } else {
        None
    };

    for filename in &["tcp", "tcp6"] {
        let tcp_path = format!("/proc/{pid}/net/{filename}");
        let content = match tokio::fs::read_to_string(&tcp_path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(unit, pid, file = filename, error = %e, "procfs tcp not readable");
                continue;
            }
        };

        for line in content.lines().skip(1) {
            // columns: sl local_address rem_address st tx_queue rx_queue tr tmwhen retrnsmt uid timeout inode
            let cols: Vec<&str> = line.split_ascii_whitespace().collect();
            if cols.len() < 10 {
                continue;
            }
            if cols[3] != "0A" {
                // Not TCP_LISTEN
                continue;
            }

            let inode: u64 = match cols[9].parse() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let owned_by_pid = match &socket_inodes {
                Some(inodes) => inodes.contains(&inode),
                None => match proc_uid {
                    // cols[7] is the socket owner UID in /proc/net/tcp
                    Some(uid) => cols[7].parse::<u32>().ok() == Some(uid),
                    // Can't determine ownership — skip rather than guess
                    None => false,
                },
            };

            if !owned_by_pid {
                continue;
            }

            if let Some(hex_port) = cols[1].rsplit(':').next() {
                if let Ok(port) = u16::from_str_radix(hex_port, 16) {
                    if port > 0 {
                        tracing::info!(
                            unit,
                            pid,
                            port,
                            inode_matched = socket_inodes.is_some(),
                            "detected listening port"
                        );
                        return Some(port);
                    }
                }
            }
        }
    }

    None
}

/// Read the effective UID of process `pid` from /proc/{pid}/status.
async fn read_proc_uid(pid: u32) -> Option<u32> {
    let content = tokio::fs::read_to_string(format!("/proc/{pid}/status"))
        .await
        .ok()?;
    for line in content.lines() {
        // "Uid:\t1000\t1000\t1000\t1000"  (real, effective, saved, fs)
        if let Some(rest) = line.strip_prefix("Uid:") {
            let mut parts = rest.split_ascii_whitespace();
            parts.next(); // skip real uid
            return parts.next()?.parse::<u32>().ok(); // effective uid
        }
    }
    None
}

/// Return the set of socket inodes open by `pid`, or `None` if
/// /proc/{pid}/fd/ is not readable (different UID, no CAP_SYS_PTRACE).
async fn collect_socket_inodes(pid: u32) -> Option<HashSet<u64>> {
    let fd_dir = format!("/proc/{pid}/fd");
    let mut dir = tokio::fs::read_dir(&fd_dir).await.ok()?;

    let mut inodes = HashSet::new();
    while let Ok(Some(entry)) = dir.next_entry().await {
        let Ok(link) = tokio::fs::read_link(entry.path()).await else {
            continue;
        };
        let s = link.to_string_lossy();
        // Symlinks for sockets look like: socket:[12345678]
        if let Some(inner) = s.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) {
            if let Ok(inode) = inner.parse::<u64>() {
                inodes.insert(inode);
            }
        }
    }

    Some(inodes)
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
    // Split on last ':' to extract port from "host:port" form
    if let Some(after_colon) = address.rsplit(':').next() {
        if let Ok(port) = after_colon.parse::<u16>() {
            if port > 0 {
                return Some(port);
            }
        }
    }
    // Entire string might be just a port number
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

    #[test]
    fn test_parse_docker_port_output_single() {
        let output = "8444/tcp -> 0.0.0.0:8444\n";
        assert_eq!(parse_docker_port_output(output), Some(8444));
    }

    #[test]
    fn test_parse_docker_port_output_multi_returns_first() {
        let output = "80/tcp -> 0.0.0.0:80\n81/tcp -> 0.0.0.0:81\n443/tcp -> 0.0.0.0:443\n8444/tcp -> 0.0.0.0:8444\n";
        assert_eq!(parse_docker_port_output(output), Some(80));
    }

    #[test]
    fn test_parse_docker_port_output_ipv6() {
        let output = "80/tcp -> :::80\n";
        assert_eq!(parse_docker_port_output(output), Some(80));
    }

    #[test]
    fn test_parse_docker_port_output_empty() {
        assert_eq!(parse_docker_port_output(""), None);
        assert_eq!(parse_docker_port_output("  \n"), None);
    }

    #[test]
    fn test_parse_docker_port_output_no_arrow() {
        assert_eq!(parse_docker_port_output("not a port line\n"), None);
    }
}
