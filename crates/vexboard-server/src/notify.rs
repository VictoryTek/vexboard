use std::collections::HashMap;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::config::NotificationsConfig;
use crate::db::models::NotificationChannel;
use crate::probe::uptime::ProbeEvent;

type HmacSha256 = Hmac<Sha256>;

/// A fully-built outgoing request, independent of any live `reqwest`
/// builder so it can be retried by rebuilding a fresh request from the
/// same owned data each attempt.
pub struct OutgoingNotification {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Builds the request for one channel, adapting the payload shape to its
/// `kind`. The `webhook` shape matches what this app has always sent,
/// byte-for-byte, so an existing downstream consumer doesn't need to change.
pub fn build_notification(
    channel: &NotificationChannel,
    event: &ProbeEvent,
    event_type: &str,
    previous_status: Option<&str>,
    config: &NotificationsConfig,
) -> OutgoingNotification {
    match channel.kind.as_str() {
        "discord" => {
            let emoji = if event.status == "down" {
                "🔴"
            } else {
                "🟢"
            };
            let body = serde_json::json!({
                "content": format!("{emoji} **{}** is {}", event.service_name, event.status),
            })
            .to_string();
            OutgoingNotification {
                url: channel.target.clone(),
                headers: vec![("Content-Type".to_string(), "application/json".to_string())],
                body,
            }
        }
        "ntfy" => {
            let (priority, tag) = if event.status == "down" {
                ("high", "warning")
            } else {
                ("default", "white_check_mark")
            };
            OutgoingNotification {
                url: channel.target.clone(),
                headers: vec![
                    ("Title".to_string(), "VexBoard".to_string()),
                    ("Priority".to_string(), priority.to_string()),
                    ("Tags".to_string(), tag.to_string()),
                ],
                body: format!("{} is {}", event.service_name, event.status),
            }
        }
        _ => {
            // "webhook" (and any unrecognized kind, as a safe fallback) — raw JSON,
            // matching the original payload shape, with optional HMAC signing.
            let payload = serde_json::json!({
                "event": event_type,
                "service_id": event.service_id,
                "service_name": event.service_name,
                "status": event.status,
                "previous_status": previous_status.unwrap_or("unknown"),
                "url": event.url,
                "latency_ms": event.latency_ms,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let body = payload.to_string();
            let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];

            let secret = channel
                .secret
                .as_deref()
                .filter(|s| !s.is_empty())
                .or_else(|| Some(config.webhook_secret.as_str()).filter(|s| !s.is_empty()));
            if let Some(s) = secret {
                let sig = hmac_sha256_hex(s, &body);
                headers.push(("X-Webhook-Signature".to_string(), format!("sha256={sig}")));
            }

            OutgoingNotification {
                url: channel.target.clone(),
                headers,
                body,
            }
        }
    }
}

/// Fires one delivery attempt and reports the real outcome — no retry, for
/// callers that need an immediate answer (the Settings "Test" button).
pub async fn send_once(
    client: &reqwest::Client,
    notification: &OutgoingNotification,
) -> Result<(), String> {
    let mut builder = client
        .post(&notification.url)
        .body(notification.body.clone());
    for (name, value) in &notification.headers {
        builder = builder.header(name, value);
    }

    match builder.send().await {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => Err(format!("destination returned {}", resp.status())),
        Err(e) => Err(e.to_string()),
    }
}

/// Wraps `send_once` with the existing retry-with-backoff behavior, for the
/// background loop where nothing is waiting on an immediate answer.
async fn send_with_retry(
    client: &reqwest::Client,
    notification: &OutgoingNotification,
    config: &NotificationsConfig,
    attempt: u32,
) {
    match send_once(client, notification).await {
        Ok(()) => {
            tracing::debug!(url = %notification.url, "Notification delivered successfully");
        }
        Err(e) => {
            if attempt < config.retry_count {
                let delay = config.retry_delay_secs * (u64::from(attempt) + 1);
                tracing::warn!(
                    url = %notification.url,
                    error = %e,
                    attempt,
                    "Notification delivery failed; retrying in {delay}s"
                );
                tokio::time::sleep(Duration::from_secs(delay)).await;
                Box::pin(send_with_retry(client, notification, config, attempt + 1)).await;
            } else {
                tracing::error!(
                    url = %notification.url,
                    error = %e,
                    "Notification delivery failed after {} attempt(s)",
                    attempt + 1
                );
            }
        }
    }
}

/// Subscribes to probe events and delivers notifications on service state transitions.
///
/// Fires only on transitions (up→down, down→up). The first probe result for each
/// service is silently recorded to avoid an alert flood at startup. Enabled
/// channels are read from the database on every transition rather than cached,
/// since transitions are infrequent and this keeps the loop simple.
pub async fn notification_loop(
    mut probe_rx: broadcast::Receiver<ProbeEvent>,
    config: NotificationsConfig,
    client: reqwest::Client,
    db: SqlitePool,
) {
    let mut prev_status: HashMap<i64, String> = HashMap::new();

    loop {
        match probe_rx.recv().await {
            Ok(event) => {
                let previous = prev_status.get(&event.service_id).cloned();
                let current = event.status.clone();

                let is_transition = previous
                    .as_deref()
                    .map(|prev| prev != current)
                    .unwrap_or(false); // first probe → no alert

                if is_transition {
                    let event_type = if current == "down" {
                        "service.down"
                    } else {
                        "service.up"
                    };

                    let channels = sqlx::query_as::<_, NotificationChannel>(
                        "SELECT id, name, kind, target, secret, events, enabled, created_at \
                         FROM notification_channels WHERE enabled = 1",
                    )
                    .fetch_all(&db)
                    .await
                    .unwrap_or_default();

                    for channel in channels {
                        let filter: Vec<String> =
                            serde_json::from_str(&channel.events).unwrap_or_default();
                        if !filter.is_empty() && !filter.iter().any(|e| e == event_type) {
                            continue;
                        }

                        let notification = build_notification(
                            &channel,
                            &event,
                            event_type,
                            previous.as_deref(),
                            &config,
                        );
                        let client_clone = client.clone();
                        let config_clone = config.clone();

                        tokio::spawn(async move {
                            send_with_retry(&client_clone, &notification, &config_clone, 0).await;
                        });
                    }
                }

                prev_status.insert(event.service_id, current);
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    dropped = n,
                    "notification loop lagged; some probe events were skipped"
                );
            }
            Err(broadcast::error::RecvError::Closed) => {
                tracing::info!("probe channel closed — notification loop exiting");
                break;
            }
        }
    }
}

fn hmac_sha256_hex(key: &str, body: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(body.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
