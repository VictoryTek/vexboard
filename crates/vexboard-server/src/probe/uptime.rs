use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::db::models::Service;

/// Event emitted when a probe completes.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeEvent {
    pub service_id: i64,
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

    // Try HEAD first; if it fails for any reason, fall back to GET.
    let (status, latency_ms) = match client.head(&url).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as i64;
            if resp.status().is_success() || resp.status().is_redirection() {
                ("up".to_string(), Some(latency))
            } else {
                ("down".to_string(), Some(latency))
            }
        }
        Err(_) => {
            // HEAD failed — fall back to GET.
            let start2 = Instant::now();
            match client.get(&url).send().await {
                Ok(resp) => {
                    let latency = start2.elapsed().as_millis() as i64;
                    if resp.status().is_success() || resp.status().is_redirection() {
                        ("up".to_string(), Some(latency))
                    } else {
                        ("down".to_string(), Some(latency))
                    }
                }
                Err(_) => ("down".to_string(), None),
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
        status,
        latency_ms,
    };
    if let Err(e) = tx.send(event) {
        tracing::debug!("no active probe subscribers: {e}");
    }
}
