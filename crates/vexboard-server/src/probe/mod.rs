pub mod uptime;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::config::ProbeConfig;

/// Spawns uptime probe tasks for all services with probing enabled.
#[tracing::instrument(skip_all)]
pub async fn start_probe_loop(
    db: SqlitePool,
    config: ProbeConfig,
    status_tx: broadcast::Sender<uptime::ProbeEvent>,
    client: reqwest::Client,
    insecure_client: reqwest::Client,
) {
    tracing::info!("Starting uptime probe scheduler");

    // The loop wakes at this short, fixed cadence to check which services are due;
    // each service's own `probe_interval` (not this constant) governs how often it
    // actually gets probed.
    const TICK_SECS: u64 = 5;
    let tick = Duration::from_secs(TICK_SECS);
    let mut last_probed: HashMap<i64, Instant> = HashMap::new();

    loop {
        // Fetch all services that have probing enabled and either a URL or a systemd unit
        let services = sqlx::query_as::<_, crate::db::models::Service>(
            "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
             sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify, created_at, updated_at \
             FROM services WHERE probe_enabled = 1 AND (url IS NOT NULL OR systemd_unit IS NOT NULL)",
        )
        .fetch_all(&db)
        .await;

        if let Ok(services) = services {
            let current_ids: HashSet<i64> = services.iter().map(|s| s.id).collect();
            last_probed.retain(|id, _| current_ids.contains(id));

            for svc in services {
                let due = last_probed.get(&svc.id).is_none_or(|t| {
                    t.elapsed() >= Duration::from_secs(svc.probe_interval.max(1) as u64)
                });
                if !due {
                    continue;
                }
                last_probed.insert(svc.id, Instant::now());

                let db = db.clone();
                let tx = status_tx.clone();
                let client = client.clone();
                let insecure_client = insecure_client.clone();
                let max_history = config.max_history;

                tokio::spawn(async move {
                    // Docker/Podman discoveries store the container name in
                    // `systemd_unit`, not a real systemd unit — only trust it as a
                    // D-Bus lookup key when the service wasn't discovered that way.
                    let use_systemd = svc.systemd_unit.is_some()
                        && !matches!(
                            svc.discovery_source.as_deref(),
                            Some("docker") | Some("podman")
                        );

                    if use_systemd {
                        uptime::probe_systemd_unit(&db, &svc, max_history, &tx).await;
                    } else if svc.url.is_some() {
                        let client = if svc.skip_tls_verify {
                            &insecure_client
                        } else {
                            &client
                        };
                        uptime::probe_service(&db, &svc, client, max_history, &tx).await;
                    }
                });
            }
        }

        tokio::time::sleep(tick).await;
    }
}
