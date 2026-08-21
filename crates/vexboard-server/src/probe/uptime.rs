use std::time::Instant;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use zbus::zvariant;

use crate::db::models::{Incident, ProbeHistoryPoint, Service, UptimeSummary};

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
    client: &reqwest::Client,
    retention_days: u64,
    tx: &broadcast::Sender<ProbeEvent>,
) {
    let url = match &svc.url {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return,
    };

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

    prune_old_results(db, svc.id, retention_days).await;

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
    retention_days: u64,
    tx: &broadcast::Sender<ProbeEvent>,
) {
    let unit_name = match &svc.systemd_unit {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return,
    };

    let start = Instant::now();
    let state_result = unit_active_state(&unit_name).await;
    let latency_ms = start.elapsed().as_millis() as i64;

    let status = match state_result {
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
            .bind(latency_ms)
            .execute(db)
            .await
    {
        tracing::error!(
            "failed to record systemd probe result for service {}: {e}",
            svc.id
        );
    }

    prune_old_results(db, svc.id, retention_days).await;

    let event = ProbeEvent {
        service_id: svc.id,
        service_name: svc.display_name.clone(),
        url: None,
        status,
        latency_ms: Some(latency_ms),
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

/// Delete probe results older than `retention_days` for one service.
async fn prune_old_results(db: &SqlitePool, service_id: i64, retention_days: u64) {
    let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::days(retention_days as i64);
    if let Err(e) = sqlx::query("DELETE FROM probe_results WHERE service_id = ? AND checked_at < ?")
        .bind(service_id)
        .bind(cutoff)
        .execute(db)
        .await
    {
        tracing::warn!("failed to prune probe history for service {service_id}: {e}");
    }
}

/// Computes 24h/7d/30d uptime percentages, a heartbeat tail, and derived
/// incidents from a chronologically-ordered (oldest-first) slice of probe
/// results. `now` is passed in rather than read internally so this stays a
/// pure, deterministically testable function.
pub fn compute_uptime_summary(rows: &[ProbeHistoryPoint], now: NaiveDateTime) -> UptimeSummary {
    let dated: Vec<(&ProbeHistoryPoint, NaiveDateTime)> = rows
        .iter()
        .filter_map(|p| p.checked_at.map(|c| (p, c)))
        .collect();

    let uptime_pct_since = |cutoff: NaiveDateTime| -> Option<f64> {
        let in_window: Vec<&&ProbeHistoryPoint> = dated
            .iter()
            .filter(|(_, checked_at)| *checked_at >= cutoff)
            .map(|(p, _)| p)
            .collect();
        if in_window.is_empty() {
            return None;
        }
        let up = in_window.iter().filter(|p| p.status == "up").count() as f64;
        Some(up / in_window.len() as f64 * 100.0)
    };

    const HEARTBEAT_COUNT: usize = 50;
    let heartbeats = rows
        .iter()
        .rev()
        .take(HEARTBEAT_COUNT)
        .rev()
        .cloned()
        .collect();

    UptimeSummary {
        uptime_24h: uptime_pct_since(now - chrono::Duration::hours(24)),
        uptime_7d: uptime_pct_since(now - chrono::Duration::days(7)),
        uptime_30d: uptime_pct_since(now - chrono::Duration::days(30)),
        heartbeats,
        incidents: derive_incidents(&dated, now),
    }
}

/// Groups consecutive non-"up" rows into incidents via a single forward scan
/// ("gaps and islands"), most-recent-first.
fn derive_incidents(
    dated: &[(&ProbeHistoryPoint, NaiveDateTime)],
    now: NaiveDateTime,
) -> Vec<Incident> {
    let mut incidents = Vec::new();
    let mut open: Option<(NaiveDateTime, i64, String)> = None; // (started_at, check_count, last_status)

    for (point, checked_at) in dated {
        let is_up = point.status == "up";
        match (&mut open, is_up) {
            (Some((_, count, last_status)), false) => {
                *count += 1;
                *last_status = point.status.clone();
            }
            (Some((started, count, last_status)), true) => {
                incidents.push(Incident {
                    status: last_status.clone(),
                    started_at: *started,
                    ended_at: Some(*checked_at),
                    duration_secs: (*checked_at - *started).num_seconds(),
                    check_count: *count,
                });
                open = None;
            }
            (None, false) => {
                open = Some((*checked_at, 1, point.status.clone()));
            }
            (None, true) => {}
        }
    }

    if let Some((started, count, last_status)) = open {
        incidents.push(Incident {
            status: last_status,
            started_at: started,
            ended_at: None,
            duration_secs: (now - started).num_seconds(),
            check_count: count,
        });
    }

    incidents.reverse();
    incidents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(status: &str, minutes_ago: i64, now: NaiveDateTime) -> ProbeHistoryPoint {
        ProbeHistoryPoint {
            status: status.to_string(),
            latency_ms: Some(10),
            checked_at: Some(now - chrono::Duration::minutes(minutes_ago)),
        }
    }

    #[test]
    fn no_incidents_when_all_up() {
        let now = chrono::Utc::now().naive_utc();
        let rows = vec![
            point("up", 10, now),
            point("up", 5, now),
            point("up", 0, now),
        ];
        let summary = compute_uptime_summary(&rows, now);
        assert!(summary.incidents.is_empty());
        assert_eq!(summary.uptime_24h, Some(100.0));
    }

    #[test]
    fn detects_a_resolved_incident_with_correct_duration() {
        let now = chrono::Utc::now().naive_utc();
        let rows = vec![
            point("up", 30, now),
            point("down", 20, now),
            point("down", 15, now),
            point("up", 10, now),
            point("up", 0, now),
        ];
        let summary = compute_uptime_summary(&rows, now);
        assert_eq!(summary.incidents.len(), 1);
        let inc = &summary.incidents[0];
        assert_eq!(inc.status, "down");
        assert_eq!(inc.check_count, 2);
        assert!(inc.ended_at.is_some());
        // recovery row is 10 min ago, first down row is 20 min ago → 10 min = 600s
        assert_eq!(inc.duration_secs, 600);
    }

    #[test]
    fn ongoing_incident_has_no_end_and_duration_to_now() {
        let now = chrono::Utc::now().naive_utc();
        let rows = vec![
            point("up", 20, now),
            point("down", 10, now),
            point("down", 0, now),
        ];
        let summary = compute_uptime_summary(&rows, now);
        assert_eq!(summary.incidents.len(), 1);
        let inc = &summary.incidents[0];
        assert!(inc.ended_at.is_none());
        assert_eq!(inc.duration_secs, 600); // now - 10min ago
    }

    #[test]
    fn unknown_and_down_rows_merge_into_one_incident() {
        let now = chrono::Utc::now().naive_utc();
        let rows = vec![
            point("up", 30, now),
            point("down", 20, now),
            point("unknown", 15, now),
            point("up", 10, now),
        ];
        let summary = compute_uptime_summary(&rows, now);
        assert_eq!(summary.incidents.len(), 1);
        assert_eq!(summary.incidents[0].check_count, 2);
        assert_eq!(summary.incidents[0].status, "unknown");
    }

    #[test]
    fn incidents_are_most_recent_first() {
        let now = chrono::Utc::now().naive_utc();
        let rows = vec![
            point("down", 50, now),
            point("up", 40, now),
            point("down", 20, now),
            point("up", 10, now),
        ];
        let summary = compute_uptime_summary(&rows, now);
        assert_eq!(summary.incidents.len(), 2);
        assert!(summary.incidents[0].started_at > summary.incidents[1].started_at);
    }

    #[test]
    fn uptime_percentage_none_when_window_has_no_data() {
        let now = chrono::Utc::now().naive_utc();
        let rows = vec![point("up", 60 * 24 * 40, now)]; // 40 days ago, outside every window
        let summary = compute_uptime_summary(&rows, now);
        assert_eq!(summary.uptime_24h, None);
        assert_eq!(summary.uptime_7d, None);
        assert_eq!(summary.uptime_30d, None);
    }

    #[test]
    fn heartbeats_are_capped_at_fifty_most_recent() {
        let now = chrono::Utc::now().naive_utc();
        let rows: Vec<ProbeHistoryPoint> = (0..80).map(|i| point("up", 80 - i, now)).collect();
        let summary = compute_uptime_summary(&rows, now);
        assert_eq!(summary.heartbeats.len(), 50);
        // oldest-first: the last element should be the most recent row (i=79, 1 min ago)
        assert_eq!(
            summary.heartbeats.last().unwrap().checked_at,
            Some(now - chrono::Duration::minutes(1))
        );
    }
}
