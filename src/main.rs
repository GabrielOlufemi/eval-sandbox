mod alerts;
mod api;
mod checker;
mod config;
mod db;
mod scheduler;
mod stats;

use std::sync::{Arc, Mutex};

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub config: Arc<Config>,
}

#[tokio::main]
async fn main() {
    let config = Config::from_env().expect("invalid configuration");
    let conn = db::open(&config.database_path).expect("failed to open database");
    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        config: Arc::new(config),
    };

    tokio::spawn(scheduler::run(state.clone()));

    let addr = format!("0.0.0.0:{}", state.config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind listener");
    println!("pulse listening on {addr}");
    axum::serve(listener, api::router(state))
        .await
        .expect("server error");
}
