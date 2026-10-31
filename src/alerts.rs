use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;

use crate::config::Config;
use crate::db::{Check, Monitor};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Down,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    pub monitor_id: i64,
    pub monitor_name: String,
    pub url: String,
    pub kind: AlertKind,
    pub at: i64,
    pub error: Option<String>,
}

/// Returns an alert when a check flips the monitor between up and down.
/// A monitor with no previous check is treated as up.
pub fn transition(monitor: &Monitor, previous: Option<&Check>, current: &Check) -> Option<Alert> {
    let was_up = previous.map(|c| c.up).unwrap_or(true);
    let kind = match (was_up, current.up) {
        (true, false) => AlertKind::Down,
        (false, true) => AlertKind::Recovered,
        _ => return None,
    };
    Some(Alert {
        monitor_id: monitor.id,
        monitor_name: monitor.name.clone(),
        url: monitor.url.clone(),
        kind,
        at: current.checked_at,
        error: current.error.clone(),
    })
}

/// Hex-encoded HMAC-SHA256 of `body`, so receivers can verify the alert came from us.
pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

pub async fn send(client: &reqwest::Client, config: &Config, alert: &Alert) {
    let Some(url) = &config.webhook_url else {
        return;
    };
    let body = serde_json::to_vec(alert).expect("alert serializes to JSON");

    let mut request = client.post(url).header("content-type", "application/json");
    if let Some(secret) = &config.webhook_secret {
        request = request.header("x-pulse-signature", format!("sha256={}", sign(secret, &body)));
    }
    if let Err(err) = request.body(body).send().await {
        eprintln!("failed to deliver alert for {}: {err}", alert.monitor_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> Monitor {
        Monitor {
            id: 1,
            name: "api".to_string(),
            url: "https://example.com".to_string(),
            expected_status: 200,
            created_at: 0,
        }
    }

    fn check(up: bool, at: i64) -> Check {
        Check {
            monitor_id: 1,
            checked_at: at,
            status_code: Some(if up { 200 } else { 503 }),
            response_ms: Some(80),
            up,
            error: None,
        }
    }

    #[test]
    fn alerts_when_monitor_goes_down() {
        let alert = transition(&monitor(), Some(&check(true, 1)), &check(false, 2)).unwrap();
        assert_eq!(alert.kind, AlertKind::Down);
        assert_eq!(alert.at, 2);
    }

    #[test]
    fn alerts_when_monitor_recovers() {
        let alert = transition(&monitor(), Some(&check(false, 1)), &check(true, 2)).unwrap();
        assert_eq!(alert.kind, AlertKind::Recovered);
    }

    #[test]
    fn no_alert_while_state_is_unchanged() {
        assert!(transition(&monitor(), Some(&check(false, 1)), &check(false, 2)).is_none());
        assert!(transition(&monitor(), Some(&check(true, 1)), &check(true, 2)).is_none());
    }

    #[test]
    fn first_check_down_alerts() {
        let alert = transition(&monitor(), None, &check(false, 1)).unwrap();
        assert_eq!(alert.kind, AlertKind::Down);
    }

    #[test]
    fn signature_depends_on_secret_and_body() {
        let base = sign("secret", b"{\"kind\":\"down\"}");
        assert_eq!(base.len(), 64);
        assert_eq!(base, sign("secret", b"{\"kind\":\"down\"}"));
        assert_ne!(base, sign("other", b"{\"kind\":\"down\"}"));
        assert_ne!(base, sign("secret", b"{\"kind\":\"recovered\"}"));
    }
}
