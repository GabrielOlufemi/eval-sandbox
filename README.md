# pulse

A small self-hosted uptime monitor. It checks a list of URLs on a fixed
interval, stores every result in SQLite, exposes uptime stats over a JSON API,
and posts a signed webhook when a monitor goes down or recovers.

## Running

```
PULSE_API_TOKEN=change-me cargo run
```

| Variable | Default | Meaning |
|---|---|---|
| `PULSE_API_TOKEN` | required | Bearer token for every endpoint except `/health` |
| `PULSE_PORT` | `8080` | HTTP port |
| `PULSE_DB` | `pulse.db` | SQLite file |
| `PULSE_CHECK_INTERVAL_SECS` | `60` | Seconds between check cycles |
| `PULSE_TIMEOUT_SECS` | `10` | Per-request timeout |
| `PULSE_WEBHOOK_URL` | unset | Where alerts are posted |
| `PULSE_WEBHOOK_SECRET` | unset | Signs alerts (`x-pulse-signature: sha256=<hex>`) |

## API

| Method | Path | |
|---|---|---|
| GET | `/health` | Liveness, no auth |
| GET / POST | `/monitors` | List or create monitors |
| GET / DELETE | `/monitors/{id}` | Fetch or delete one monitor |
| GET | `/monitors/{id}/stats?window=24h` | Uptime % and average response time |
