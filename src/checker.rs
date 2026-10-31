use std::time::{Duration, Instant};

use crate::db::{Check, Monitor};

pub fn build_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .expect("failed to build HTTP client")
}

/// Requests the monitor's URL once and records how it went.
pub async fn check(client: &reqwest::Client, monitor: &Monitor, now: i64) -> Check {
    let started = Instant::now();
    match client.get(&monitor.url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            Check {
                monitor_id: monitor.id,
                checked_at: now,
                status_code: Some(status),
                response_ms: Some(started.elapsed().as_millis() as i64),
                up: status == monitor.expected_status,
                error: None,
            }
        }
        Err(err) => Check {
            monitor_id: monitor.id,
            checked_at: now,
            status_code: None,
            response_ms: None,
            up: false,
            error: Some(err.to_string()),
        },
    }
}
