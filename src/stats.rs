use serde::Serialize;

use crate::db::Check;

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total_checks: usize,
    pub uptime_percent: f64,
    pub avg_response_ms: Option<f64>,
}

pub fn compute(checks: &[Check]) -> Stats {
    if checks.is_empty() {
        return Stats {
            total_checks: 0,
            uptime_percent: 100.0,
            avg_response_ms: None,
        };
    }

    let up = checks.iter().filter(|c| c.up).count();
    let times: Vec<i64> = checks.iter().filter_map(|c| c.response_ms).collect();
    let avg_response_ms = if times.is_empty() {
        None
    } else {
        Some(times.iter().sum::<i64>() as f64 / times.len() as f64)
    };

    Stats {
        total_checks: checks.len(),
        uptime_percent: up as f64 * 100.0 / checks.len() as f64,
        avg_response_ms,
    }
}

/// Parses a window such as "30m" or "24h" into seconds.
pub fn parse_window(raw: &str) -> Option<i64> {
    if !raw.is_ascii() || raw.len() < 2 {
        return None;
    }
    let (amount, unit) = raw.split_at(raw.len() - 1);
    let amount: i64 = amount.parse().ok()?;
    if amount <= 0 {
        return None;
    }
    let unit_secs = match unit {
        "m" => 60,
        "h" => 60 * 60,
        _ => return None,
    };
    amount.checked_mul(unit_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(up: bool, response_ms: Option<i64>) -> Check {
        Check {
            monitor_id: 1,
            checked_at: 0,
            status_code: None,
            response_ms,
            up,
            error: None,
        }
    }

    #[test]
    fn no_checks_means_fully_up() {
        let stats = compute(&[]);
        assert_eq!(stats.total_checks, 0);
        assert_eq!(stats.uptime_percent, 100.0);
        assert_eq!(stats.avg_response_ms, None);
    }

    #[test]
    fn uptime_is_share_of_up_checks() {
        let checks = [check(true, Some(100)), check(false, None), check(true, Some(300)), check(true, Some(200))];
        let stats = compute(&checks);
        assert_eq!(stats.total_checks, 4);
        assert_eq!(stats.uptime_percent, 75.0);
    }

    #[test]
    fn average_ignores_checks_without_timing() {
        let checks = [check(true, Some(100)), check(false, None), check(true, Some(300))];
        assert_eq!(compute(&checks).avg_response_ms, Some(200.0));
    }

    #[test]
    fn parses_minute_and_hour_windows() {
        assert_eq!(parse_window("30m"), Some(1_800));
        assert_eq!(parse_window("24h"), Some(86_400));
    }

    #[test]
    fn rejects_bad_windows() {
        for raw in ["", "h", "0h", "-5m", "10x", "1.5h", "5é"] {
            assert_eq!(parse_window(raw), None, "{raw:?} should be rejected");
        }
    }
}
