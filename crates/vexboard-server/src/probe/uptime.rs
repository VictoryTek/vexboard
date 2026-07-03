use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use zbus::zvariant;

use crate::db::models::Service;

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
    fn list_units(&self) -> zbus::Result<Vec<SystemdUnitInfo>>;
}

#[derive(Debug, zvariant::Type, Deserialize)]
#[allow(dead_code)]
struct SystemdUnitInfo {
    name: String,
    description: String,
    load_state: String,
    active_state: String,
    sub_state: String,
    followed: String,
    object_path: zvariant::OwnedObjectPath,
    queued_job_id: u32,
    job_type: String,
    job_object_path: zvariant::OwnedObjectPath,
}

/// Event emitted when a probe completes.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeEvent {
    pub service_id: i64,
    pub service_name: String,
    pub url: Option<String>,
    pub status: String,
    pub latency_ms: Option<i64>,
}

/// Probe a single service's URL and record the result.
#[tracing::instrument(skip(db, tx), fields(service_id = svc.id, url = ?svc.url))]
pub async fn probe_service(
    db: &SqlitePool,
    svc: &Service,
    timeout: Duration,
    max_history: u64,
    tx: &broadcast::Sender<ProbeEvent>,
) {
    let url = match &svc.url {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return,
    };

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(false)
        .build()
        .unwrap_or_default();

    let start = Instant::now();

    // Try HEAD first; fall back to GET whenever HEAD doesn't come back up
    // (request error, or a non-success status — many servers don't implement
    // HEAD and would otherwise be misreported as down).
    let head_outcome = client.head(&url).send().await;
    let (status, latency_ms) = match head_outcome {
        Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {
            ("up".to_string(), Some(start.elapsed().as_millis() as i64))
        }
        other => {
            match &other {
                Ok(resp) => tracing::debug!(
                    url = %url, status = %resp.status(),
                    "HEAD probe returned non-success status, falling back to GET"
                ),
                Err(e) => tracing::debug!(
                    url = %url, error = %e,
                    "HEAD probe request failed, falling back to GET"
                ),
            }
            let start2 = Instant::now();
            match client.get(&url).send().await {
                Ok(resp) => {
                    let latency = start2.elapsed().as_millis() as i64;
                    if resp.status().is_success() || resp.status().is_redirection() {
                        ("up".to_string(), Some(latency))
                    } else {
                        tracing::warn!(
                            url = %url, status = %resp.status(),
                            "GET probe returned non-success status, marking service down"
                        );
                        ("down".to_string(), Some(latency))
                    }
                }
                Err(e) => {
                    tracing::warn!(url = %url, error = %e, "GET probe failed, marking service down");
                    ("down".to_string(), None)
                }
            }
        }
    };

    // Record result in database
    if let Err(e) =
        sqlx::query("INSERT INTO probe_results (service_id, status, latency_ms) VALUES (?, ?, ?)")
            .bind(svc.id)
            .bind(&status)
            .bind(latency_ms)
            .execute(db)
            .await
    {
        tracing::error!("failed to record probe result for service {}: {e}", svc.id);
    }

    // Trim old results to keep max_history entries
    if let Err(e) = sqlx::query(
        "DELETE FROM probe_results WHERE service_id = ? AND id NOT IN \
         (SELECT id FROM probe_results WHERE service_id = ? ORDER BY checked_at DESC LIMIT ?)",
    )
    .bind(svc.id)
    .bind(svc.id)
    .bind(max_history as i64)
    .execute(db)
    .await
    {
        tracing::warn!("failed to trim probe history for service {}: {e}", svc.id);
    }

    // Broadcast event
    let event = ProbeEvent {
        service_id: svc.id,
        service_name: svc.display_name.clone(),
        url: svc.url.clone(),
        status,
        latency_ms,
    };
    if let Err(e) = tx.send(event) {
        tracing::debug!("no active probe subscribers: {e}");
    }
}

/// Probe a systemd unit's active state via D-Bus and record the result.
#[tracing::instrument(skip(db, tx), fields(service_id = svc.id, unit = ?svc.systemd_unit))]
pub async fn probe_systemd_unit(
    db: &SqlitePool,
    svc: &Service,
    max_history: u64,
    tx: &broadcast::Sender<ProbeEvent>,
) {
    let unit_name = match &svc.systemd_unit {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return,
    };

    let status = match unit_active_state(&unit_name).await {
        Ok(state) => match state.as_str() {
            "active" | "reloading" | "activating" => "up".to_string(),
            _ => "down".to_string(),
        },
        Err(e) => {
            tracing::warn!(unit = %unit_name, "D-Bus unit state query failed: {e}");
            "down".to_string()
        }
    };

    if let Err(e) =
        sqlx::query("INSERT INTO probe_results (service_id, status, latency_ms) VALUES (?, ?, ?)")
            .bind(svc.id)
            .bind(&status)
            .bind(None::<i64>)
            .execute(db)
            .await
    {
        tracing::error!(
            "failed to record systemd probe result for service {}: {e}",
            svc.id
        );
    }

    if let Err(e) = sqlx::query(
        "DELETE FROM probe_results WHERE service_id = ? AND id NOT IN \
         (SELECT id FROM probe_results WHERE service_id = ? ORDER BY checked_at DESC LIMIT ?)",
    )
    .bind(svc.id)
    .bind(svc.id)
    .bind(max_history as i64)
    .execute(db)
    .await
    {
        tracing::warn!("failed to trim probe history for service {}: {e}", svc.id);
    }

    let event = ProbeEvent {
        service_id: svc.id,
        service_name: svc.display_name.clone(),
        url: None,
        status,
        latency_ms: None,
    };
    if let Err(e) = tx.send(event) {
        tracing::debug!("no active probe subscribers: {e}");
    }
}

async fn unit_active_state(unit_name: &str) -> anyhow::Result<String> {
    let conn = zbus::Connection::system().await?;
    let proxy = SystemdManagerProxy::new(&conn).await?;
    let units = proxy.list_units().await?;
    for unit in units {
        if unit.name == unit_name {
            return Ok(unit.active_state);
        }
    }
    // Unit not in the loaded list → treat as inactive
    Ok("inactive".to_string())
}
