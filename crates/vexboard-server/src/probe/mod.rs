pub mod uptime;

use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::config::ProbeConfig;

/// Spawns uptime probe tasks for all services with probing enabled.
#[tracing::instrument(skip_all)]
pub async fn start_probe_loop(
    db: SqlitePool,
    config: ProbeConfig,
    status_tx: broadcast::Sender<uptime::ProbeEvent>,
) {
    tracing::info!("Starting uptime probe scheduler");

    let interval = Duration::from_secs(config.default_interval_secs);

    loop {
        // Fetch all services that have probing enabled and either a URL or a systemd unit
        let services = sqlx::query_as::<_, crate::db::models::Service>(
            "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
             sort_order, probe_enabled, probe_interval, tags, visible, created_at, updated_at \
             FROM services WHERE probe_enabled = 1 AND (url IS NOT NULL OR systemd_unit IS NOT NULL)",
        )
        .fetch_all(&db)
        .await;

        if let Ok(services) = services {
            for svc in services {
                let db = db.clone();
                let tx = status_tx.clone();
                let timeout = Duration::from_secs(config.timeout_secs);
                let max_history = config.max_history;

                tokio::spawn(async move {
                    if svc.systemd_unit.is_some() {
                        uptime::probe_systemd_unit(&db, &svc, max_history, &tx).await;
                    } else if svc.url.is_some() {
                        uptime::probe_service(&db, &svc, timeout, max_history, &tx).await;
                    }
                });
            }
        }

        tokio::time::sleep(interval).await;
    }
}
