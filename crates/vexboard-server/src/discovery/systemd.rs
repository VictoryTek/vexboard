use std::time::Duration;

use sqlx::SqlitePool;
use zbus::Connection;

use crate::config::DiscoveryConfig;
use crate::discovery::{DiscoveredUnit, DiscoveryList};

/// Background loop that periodically discovers systemd services via D-Bus.
#[tracing::instrument(skip_all)]
pub async fn discovery_loop(
    discoveries: DiscoveryList,
    db: SqlitePool,
    config: DiscoveryConfig,
) {
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
    let proxy = zbus::fdo::ManagerProxy::builder(&connection)
        .destination("org.freedesktop.systemd1")?
        .path("/org/freedesktop/systemd1")?
        .build()
        .await?;

    // ListUnits returns Vec of unit structs
    let units = proxy.list_units().await?;

    let mut unclaimed = Vec::new();

    for unit in &units {
        let name = &unit.0;      // unit name
        let desc = &unit.1;      // description
        let load_state = &unit.2; // load state
        let active_state = &unit.3; // active state
        let sub_state = &unit.4; // sub state

        // Only .service units that are loaded and active
        if !name.ends_with(".service") {
            continue;
        }
        if load_state != "loaded" || active_state != "active" {
            continue;
        }

        // Check exclusion patterns
        if is_excluded(name, &config.exclude_units) {
            continue;
        }

        // Check if already claimed in DB
        let claimed = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM services WHERE systemd_unit = ?",
        )
        .bind(name)
        .fetch_one(db)
        .await
        .unwrap_or(0);

        if claimed > 0 {
            continue;
        }

        unclaimed.push(DiscoveredUnit {
            unit_name: name.clone(),
            description: desc.clone(),
            active_state: active_state.clone(),
            sub_state: sub_state.clone(),
        });
    }

    // Update the shared discovery list
    let mut list = discoveries.write().await;
    *list = unclaimed;
    tracing::debug!(count = list.len(), "Discovery pass complete");

    Ok(())
}

/// Check if a unit name matches any exclusion pattern (supports trailing `*` glob).
fn is_excluded(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            if name.starts_with(prefix) {
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
}
