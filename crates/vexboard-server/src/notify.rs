use std::collections::HashMap;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::sync::broadcast;

use crate::config::NotificationsConfig;
use crate::probe::uptime::ProbeEvent;

type HmacSha256 = Hmac<Sha256>;

/// Subscribes to probe events and delivers webhooks on service state transitions.
///
/// Fires only on transitions (up→down, down→up). The first probe result for each
/// service is silently recorded to avoid an alert flood at startup.
pub async fn notification_loop(
    mut probe_rx: broadcast::Receiver<ProbeEvent>,
    config: NotificationsConfig,
    client: reqwest::Client,
) {
    if config.webhooks.is_empty() {
        tracing::debug!("No webhooks configured — notification loop idle");
    }

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

                if is_transition && !config.webhooks.is_empty() {
                    let event_type = if current == "down" {
                        "service.down"
                    } else {
                        "service.up"
                    };

                    let payload = serde_json::json!({
                        "event": event_type,
                        "service_id": event.service_id,
                        "service_name": event.service_name,
                        "status": current,
                        "previous_status": previous.as_deref().unwrap_or("unknown"),
                        "url": event.url,
                        "latency_ms": event.latency_ms,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });

                    let body = payload.to_string();

                    for webhook in &config.webhooks {
                        if !webhook.events.is_empty()
                            && !webhook.events.iter().any(|e| e == event_type)
                        {
                            continue;
                        }

                        let secret = if !webhook.secret.is_empty() {
                            Some(webhook.secret.as_str())
                        } else if !config.webhook_secret.is_empty() {
                            Some(config.webhook_secret.as_str())
                        } else {
                            None
                        };

                        let webhook_url = webhook.url.clone();
                        let body_clone = body.clone();
                        let secret_owned = secret.map(|s| s.to_owned());
                        let client_clone = client.clone();
                        let config_clone = config.clone();

                        tokio::spawn(async move {
                            fire_webhook(
                                &client_clone,
                                &webhook_url,
                                &body_clone,
                                secret_owned.as_deref(),
                                &config_clone,
                                0,
                            )
                            .await;
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

async fn fire_webhook(
    client: &reqwest::Client,
    url: &str,
    body: &str,
    secret: Option<&str>,
    config: &NotificationsConfig,
    attempt: u32,
) {
    let mut builder = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.to_owned());

    if let Some(s) = secret {
        let sig = hmac_sha256_hex(s, body);
        builder = builder.header("X-Webhook-Signature", format!("sha256={sig}"));
    }

    match builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(url, "Webhook delivered successfully");
        }
        Ok(resp) => {
            let status = resp.status();
            if attempt < config.retry_count {
                let delay = config.retry_delay_secs * (u64::from(attempt) + 1);
                tracing::warn!(
                    url,
                    %status,
                    attempt,
                    "Webhook delivery failed; retrying in {delay}s"
                );
                tokio::time::sleep(Duration::from_secs(delay)).await;
                Box::pin(fire_webhook(client, url, body, secret, config, attempt + 1)).await;
            } else {
                tracing::error!(
                    url,
                    %status,
                    "Webhook delivery failed after {} attempt(s)",
                    attempt + 1
                );
            }
        }
        Err(e) => {
            if attempt < config.retry_count {
                let delay = config.retry_delay_secs * (u64::from(attempt) + 1);
                tracing::warn!(
                    url,
                    error = %e,
                    attempt,
                    "Webhook request failed; retrying in {delay}s"
                );
                tokio::time::sleep(Duration::from_secs(delay)).await;
                Box::pin(fire_webhook(client, url, body, secret, config, attempt + 1)).await;
            } else {
                tracing::error!(
                    url,
                    error = %e,
                    "Webhook delivery failed after {} attempt(s)",
                    attempt + 1
                );
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
