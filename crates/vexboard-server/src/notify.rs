use std::collections::HashMap;
use std::time::{Duration, Instant};

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
        "telegram" => {
            let token = channel.secret.as_deref().unwrap_or_default();
            let emoji = if event.status == "down" {
                "🔴"
            } else {
                "🟢"
            };
            let body = serde_json::json!({
                "chat_id": channel.target,
                "text": format!("{emoji} *{}* is {}", event.service_name, event.status),
                "parse_mode": "Markdown",
            })
            .to_string();
            OutgoingNotification {
                url: format!("https://api.telegram.org/bot{token}/sendMessage"),
                headers: vec![("Content-Type".to_string(), "application/json".to_string())],
                body,
            }
        }
        "gotify" => {
            let token = channel.secret.as_deref().unwrap_or_default();
            let priority = if event.status == "down" { 8 } else { 2 };
            let body = serde_json::json!({
                "title": "VexBoard",
                "message": format!("{} is {}", event.service_name, event.status),
                "priority": priority,
            })
            .to_string();
            OutgoingNotification {
                url: format!("{}/message", channel.target.trim_end_matches('/')),
                headers: vec![
                    ("Content-Type".to_string(), "application/json".to_string()),
                    ("X-Gotify-Key".to_string(), token.to_string()),
                ],
                body,
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

/// Per-service alert bookkeeping — how long it's been failing, and whether/when
/// this outage actually triggered a notification (as opposed to a blip that
/// never crossed the failure threshold).
struct ServiceAlertState {
    /// The status as of the previous probe — kept only so the `webhook` kind's
    /// `previous_status` payload field stays accurate; the fire/repeat logic
    /// below is driven entirely by `consecutive_down`/`notified_down`.
    last_status: String,
    consecutive_down: i64,
    notified_down: bool,
    last_notified_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FireDecision {
    None,
    Down,
    Up,
}

/// The alert-rules decision, factored out as a pure function (given `now`
/// explicitly rather than reading the clock) so it's directly unit-testable
/// without a broadcast channel or a database. Mutates `state` in place —
/// same reasoning as `probe::uptime::compute_uptime_summary`: this is the one
/// piece of genuinely non-obvious logic in the feature, so it earns its own
/// tests independent of the loop that drives it.
fn decide_fire(
    status: &str,
    state: &mut ServiceAlertState,
    fail_threshold: i64,
    repeat_interval_mins: i64,
    now: Instant,
) -> FireDecision {
    if status == "down" {
        state.consecutive_down += 1;

        let should_fire = if !state.notified_down {
            state.consecutive_down >= fail_threshold.max(1)
        } else {
            repeat_interval_mins > 0
                && state
                    .last_notified_at
                    .map(|t| {
                        now.duration_since(t)
                            >= Duration::from_secs((repeat_interval_mins * 60) as u64)
                    })
                    .unwrap_or(false)
        };

        if !should_fire {
            FireDecision::None
        } else {
            state.notified_down = true;
            state.last_notified_at = Some(now);
            FireDecision::Down
        }
    } else {
        let was_notified = state.notified_down;
        state.consecutive_down = 0;
        state.notified_down = false;
        state.last_notified_at = None;
        // Only announce recovery if this outage actually alerted — a blip
        // that never crossed the threshold said nothing on the way down,
        // so it says nothing on the way back up either.
        if was_notified {
            FireDecision::Up
        } else {
            FireDecision::None
        }
    }
}

/// Subscribes to probe events and delivers notifications according to the
/// configured alert rules (`notify_fail_threshold` / `notify_repeat_interval_mins`
/// in the `settings` table, editable from the Notifications settings tab).
///
/// With the defaults (threshold 1, interval 0) this reproduces the original
/// transition-only behavior exactly: the first failed probe alerts
/// immediately, and a still-down service never gets a repeat. The first probe
/// result for each service is always silently recorded (no alert), matching
/// the original loop, so a fresh service can't fire an alert from unknown → down.
pub async fn notification_loop(
    mut probe_rx: broadcast::Receiver<ProbeEvent>,
    config: NotificationsConfig,
    client: reqwest::Client,
    db: SqlitePool,
) {
    let mut states: HashMap<i64, ServiceAlertState> = HashMap::new();

    loop {
        match probe_rx.recv().await {
            Ok(event) => {
                let is_first_observation = !states.contains_key(&event.service_id);
                let state = states.entry(event.service_id).or_insert(ServiceAlertState {
                    last_status: event.status.clone(),
                    consecutive_down: 0,
                    notified_down: false,
                    last_notified_at: None,
                });

                if !is_first_observation {
                    let previous_status = state.last_status.clone();
                    state.last_status = event.status.clone();

                    let fail_threshold = fetch_setting_i64(&db, "notify_fail_threshold", 1).await;
                    let repeat_interval_mins =
                        fetch_setting_i64(&db, "notify_repeat_interval_mins", 0).await;
                    let decision = decide_fire(
                        &event.status,
                        state,
                        fail_threshold,
                        repeat_interval_mins,
                        Instant::now(),
                    );
                    let event_type = match decision {
                        FireDecision::None => None,
                        FireDecision::Down => Some("service.down"),
                        FireDecision::Up => Some("service.up"),
                    };

                    if let Some(event_type) = event_type {
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
                                Some(&previous_status),
                                &config,
                            );
                            let client_clone = client.clone();
                            let config_clone = config.clone();

                            tokio::spawn(async move {
                                send_with_retry(&client_clone, &notification, &config_clone, 0)
                                    .await;
                            });
                        }
                    }
                }
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

async fn fetch_setting_i64(db: &SqlitePool, key: &str, default: i64) -> i64 {
    crate::db::get_setting(db, key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_state() -> ServiceAlertState {
        ServiceAlertState {
            last_status: "up".to_string(),
            consecutive_down: 0,
            notified_down: false,
            last_notified_at: None,
        }
    }

    #[test]
    fn default_threshold_fires_on_first_failure() {
        let mut state = fresh_state();
        let decision = decide_fire("down", &mut state, 1, 0, Instant::now());
        assert_eq!(decision, FireDecision::Down);
        assert!(state.notified_down);
    }

    #[test]
    fn higher_threshold_waits_for_consecutive_failures() {
        let mut state = fresh_state();
        let now = Instant::now();
        assert_eq!(
            decide_fire("down", &mut state, 3, 0, now),
            FireDecision::None
        );
        assert_eq!(
            decide_fire("down", &mut state, 3, 0, now),
            FireDecision::None
        );
        assert_eq!(
            decide_fire("down", &mut state, 3, 0, now),
            FireDecision::Down
        );
        assert!(state.notified_down);
    }

    #[test]
    fn zero_repeat_interval_never_fires_again_while_down() {
        let mut state = fresh_state();
        let now = Instant::now();
        assert_eq!(
            decide_fire("down", &mut state, 1, 0, now),
            FireDecision::Down
        );
        let later = now + Duration::from_secs(3600);
        assert_eq!(
            decide_fire("down", &mut state, 1, 0, later),
            FireDecision::None,
            "repeat_interval_mins = 0 must mean 'never repeat'"
        );
    }

    #[test]
    fn repeat_interval_fires_again_once_elapsed() {
        let mut state = fresh_state();
        let now = Instant::now();
        assert_eq!(
            decide_fire("down", &mut state, 1, 30, now),
            FireDecision::Down
        );

        let too_soon = now + Duration::from_secs(10 * 60);
        assert_eq!(
            decide_fire("down", &mut state, 1, 30, too_soon),
            FireDecision::None
        );

        let elapsed = now + Duration::from_secs(31 * 60);
        assert_eq!(
            decide_fire("down", &mut state, 1, 30, elapsed),
            FireDecision::Down
        );
    }

    #[test]
    fn recovery_only_announced_if_outage_actually_alerted() {
        // Threshold of 3, but only 2 consecutive failures — never crossed the
        // threshold, so nothing was ever said about the outage. Recovery must
        // stay silent too, matching what was (not) said on the way down.
        let mut state = fresh_state();
        let now = Instant::now();
        assert_eq!(
            decide_fire("down", &mut state, 3, 0, now),
            FireDecision::None
        );
        assert_eq!(
            decide_fire("down", &mut state, 3, 0, now),
            FireDecision::None
        );
        assert_eq!(decide_fire("up", &mut state, 3, 0, now), FireDecision::None);
    }

    #[test]
    fn recovery_announced_after_a_real_alerted_outage() {
        let mut state = fresh_state();
        let now = Instant::now();
        assert_eq!(
            decide_fire("down", &mut state, 1, 0, now),
            FireDecision::Down
        );
        assert_eq!(decide_fire("up", &mut state, 1, 0, now), FireDecision::Up);
        // State fully resets after recovery — a fresh outage starts from zero.
        assert_eq!(state.consecutive_down, 0);
        assert!(!state.notified_down);
        assert!(state.last_notified_at.is_none());
    }
}
