use std::env;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

pub struct Config {
    pub port: u16,
    pub database_path: String,
    pub check_interval: Duration,
    pub request_timeout: Duration,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub api_token: String,
}

#[derive(Debug)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Config {
            port: parse_or("PULSE_PORT", 8080)?,
            database_path: env::var("PULSE_DB").unwrap_or_else(|_| "pulse.db".to_string()),
            check_interval: Duration::from_secs(parse_or("PULSE_CHECK_INTERVAL_SECS", 60)?),
            request_timeout: Duration::from_secs(parse_or("PULSE_TIMEOUT_SECS", 10)?),
            webhook_url: env::var("PULSE_WEBHOOK_URL").ok(),
            webhook_secret: env::var("PULSE_WEBHOOK_SECRET").ok(),
            api_token: env::var("PULSE_API_TOKEN")
                .map_err(|_| ConfigError("PULSE_API_TOKEN must be set".to_string()))?,
        })
    }
}

fn parse_or<T: FromStr>(key: &str, default: T) -> Result<T, ConfigError> {
    match env::var(key) {
        Ok(raw) => raw
            .parse()
            .map_err(|_| ConfigError(format!("{key} has an invalid value: {raw}"))),
        Err(_) => Ok(default),
    }
}
