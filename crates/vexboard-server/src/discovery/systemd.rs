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

        // Check exclusion patterns
        if is_excluded(name, &config.exclude_units) {
            continue;
        }

        // Check if already claimed in DB
        let claimed =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM services WHERE systemd_unit = ?")
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
            source: "systemd".to_string(),
            url_hint: None,
        });
    }

    // Replace only systemd entries, preserving container discoveries
    let mut list = discoveries.write().await;
    list.retain(|u| u.source != "systemd");
    list.extend(unclaimed);
    tracing::debug!(count = list.len(), "systemd discovery pass complete");

    Ok(())
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
}
