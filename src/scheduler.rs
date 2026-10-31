use std::time::{SystemTime, UNIX_EPOCH};

use tokio::task::JoinSet;

use crate::{alerts, checker, db, AppState};

pub async fn run(state: AppState) {
    let client = checker::build_client(state.config.request_timeout);
    let mut ticker = tokio::time::interval(state.config.check_interval);
    loop {
        ticker.tick().await;
        if let Err(err) = run_once(&state, &client).await {
            eprintln!("check cycle failed: {err}");
        }
    }
}

/// Checks every monitor concurrently, stores the results and sends alerts.
async fn run_once(state: &AppState, client: &reqwest::Client) -> rusqlite::Result<()> {
    let monitors = {
        let conn = state.db.lock().unwrap();
        db::list_monitors(&conn)?
    };

    let now = unix_now();
    let mut pending = JoinSet::new();
    for monitor in monitors {
        let client = client.clone();
        pending.spawn(async move {
            let check = checker::check(&client, &monitor, now).await;
            (monitor, check)
        });
    }

    while let Some(joined) = pending.join_next().await {
        let (monitor, check) = match joined {
            Ok(result) => result,
            Err(err) => {
                eprintln!("check task panicked: {err}");
                continue;
            }
        };

        let previous = {
            let conn = state.db.lock().unwrap();
            let previous = db::last_check(&conn, monitor.id)?;
            db::insert_check(&conn, &check)?;
            previous
        };

        if let Some(alert) = alerts::transition(&monitor, previous.as_ref(), &check) {
            alerts::send(client, &state.config, &alert).await;
        }
    }
    Ok(())
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before 1970")
        .as_secs() as i64
}
