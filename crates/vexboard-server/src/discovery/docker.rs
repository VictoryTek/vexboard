use std::time::Duration;

use bollard::container::ListContainersOptions;
use bollard::Docker;
use sqlx::SqlitePool;

use crate::config::DockerConfig;
use crate::discovery::{DiscoveredUnit, DiscoveryList};

/// Background loop that periodically discovers running containers via the Docker/Podman API.
#[tracing::instrument(skip_all)]
pub async fn docker_discovery_loop(
    discoveries: DiscoveryList,
    db: SqlitePool,
    config: DockerConfig,
) {
    if !config.enabled {
        tracing::info!("container discovery is disabled");
        return;
    }

    let interval = Duration::from_secs(config.interval_secs);
    tracing::info!(?interval, "Starting container discovery loop");

    loop {
        if let Err(e) = discover_containers(&discoveries, &db, &config).await {
            tracing::warn!("Container discovery pass failed: {e}");
        }
        tokio::time::sleep(interval).await;
    }
}

/// Perform a single discovery pass across all configured sockets.
pub async fn discover_containers(
    discoveries: &DiscoveryList,
    db: &SqlitePool,
    config: &DockerConfig,
) -> anyhow::Result<()> {
    let mut all = Vec::new();

    for socket in &config.sockets {
        if !std::path::Path::new(socket).exists() {
            continue;
        }
        let source = if socket.contains("podman") {
            "podman"
        } else {
            "docker"
        };
        match discover_from_socket(socket, source, db, &config.exclude_images).await {
            Ok(mut units) => all.append(&mut units),
            Err(e) => tracing::debug!(%socket, "socket query failed: {e}"),
        }
    }

    let mut list = discoveries.write().await;
    list.retain(|u| u.source != "docker" && u.source != "podman");
    list.extend(all);
    tracing::debug!(count = list.len(), "container discovery pass complete");
    Ok(())
}

/// Returns the host to use in URL hints for containers reached via `socket`.
/// Unix socket paths → "localhost"; TCP/HTTP URLs → the hostname segment.
fn socket_host(socket: &str) -> &str {
    if socket.starts_with('/') {
        return "localhost";
    }
    // Strip scheme prefix and take the host portion before any colon.
    let after_scheme = socket
        .strip_prefix("tcp://")
        .or_else(|| socket.strip_prefix("http://"))
        .unwrap_or(socket);
    after_scheme.split(':').next().unwrap_or("localhost")
}

async fn discover_from_socket(
    socket: &str,
    source: &str,
    db: &SqlitePool,
    exclude_images: &[String],
) -> anyhow::Result<Vec<DiscoveredUnit>> {
    let host = socket_host(socket);
    let docker = Docker::connect_with_socket(socket, 10, bollard::API_DEFAULT_VERSION)?;

    // Quick connectivity check
    docker.ping().await?;

    let containers = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        }))
        .await?;

    let mut units = Vec::new();

    for c in containers {
        let name = c
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| {
                c.id.as_deref()
                    .unwrap_or("unknown")
                    .chars()
                    .take(12)
                    .collect()
            });

        let image = c.image.as_deref().unwrap_or("unknown").to_string();
        let status = c.status.as_deref().unwrap_or("running").to_string();

        // Honour opt-out label
        if c.labels
            .as_ref()
            .and_then(|l| l.get("vexboard.ignore"))
            .map(|v| v == "true")
            .unwrap_or(false)
        {
            continue;
        }

        // Skip excluded images
        if exclude_images.iter().any(|e| image.contains(e.as_str())) {
            continue;
        }

        // Skip if already claimed by either display_name or systemd_unit.
        let claimed = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM services WHERE display_name = ? OR systemd_unit = ? LIMIT 1)",
        )
        .bind(&name)
        .bind(&name)
        .fetch_one(db)
        .await
        .unwrap_or(false);

        if claimed {
            continue;
        }

        // Find first mapped public port for URL hint.
        // Prefer the port's bound IP when it's a specific address; fall back
        // to the host derived from the socket (localhost for Unix sockets,
        // the remote hostname for TCP endpoints).
        let url_hint = c.ports.as_ref().and_then(|ports| {
            ports
                .iter()
                .find(|p| p.public_port.is_some())
                .and_then(|p| {
                    let port = p.public_port?;
                    let bound = p.ip.as_deref().unwrap_or("");
                    let h = if bound.is_empty() || bound == "0.0.0.0" || bound == "::" {
                        host
                    } else {
                        bound
                    };
                    Some(format!("http://{h}:{port}"))
                })
        });

        units.push(DiscoveredUnit {
            unit_name: name,
            description: format!("{image} — {status}"),
            active_state: "active".to_string(),
            sub_state: status,
            source: source.to_string(),
            url_hint,
        });
    }

    Ok(units)
}
