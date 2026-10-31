use std::sync::MutexGuard;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::scheduler::unix_now;
use crate::{db, stats, AppState};

const DEFAULT_WINDOW: &str = "24h";

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/monitors", get(list_monitors).post(create_monitor))
        .route("/monitors/{id}", get(get_monitor).delete(delete_monitor))
        .route("/monitors/{id}/stats", get(monitor_stats))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(protected)
        .with_state(state)
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let expected = format!("Bearer {}", state.config.api_token);
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);

    if authorized {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

pub enum ApiError {
    NotFound,
    BadRequest(String),
    Internal(rusqlite::Error),
}

impl From<rusqlite::Error> for ApiError {
    fn from(err: rusqlite::Error) -> Self {
        ApiError::Internal(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND.into_response(),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            ApiError::Internal(err) => {
                eprintln!("database error: {err}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

fn conn(state: &AppState) -> MutexGuard<'_, Connection> {
    state.db.lock().unwrap()
}

async fn list_monitors(State(state): State<AppState>) -> Result<Json<Vec<db::Monitor>>, ApiError> {
    Ok(Json(db::list_monitors(&conn(&state))?))
}

#[derive(Deserialize)]
struct NewMonitor {
    name: String,
    url: String,
    expected_status: Option<u16>,
}

async fn create_monitor(
    State(state): State<AppState>,
    Json(input): Json<NewMonitor>,
) -> Result<(StatusCode, Json<db::Monitor>), ApiError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".to_string()));
    }
    if !(input.url.starts_with("http://") || input.url.starts_with("https://")) {
        return Err(ApiError::BadRequest("url must start with http:// or https://".to_string()));
    }
    let expected_status = input.expected_status.unwrap_or(200);

    let monitor = db::insert_monitor(&conn(&state), name, &input.url, expected_status, unix_now())?;
    Ok((StatusCode::CREATED, Json(monitor)))
}

async fn get_monitor(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<db::Monitor>, ApiError> {
    db::get_monitor(&conn(&state), id)?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn delete_monitor(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    if db::delete_monitor(&conn(&state), id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize)]
struct StatsQuery {
    window: Option<String>,
}

#[derive(Serialize)]
struct StatsResponse {
    monitor_id: i64,
    window: String,
    #[serde(flatten)]
    stats: stats::Stats,
}

async fn monitor_stats(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, ApiError> {
    let window = query.window.unwrap_or_else(|| DEFAULT_WINDOW.to_string());
    let seconds = stats::parse_window(&window)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid window: {window}")))?;

    let conn = conn(&state);
    if db::get_monitor(&conn, id)?.is_none() {
        return Err(ApiError::NotFound);
    }
    let checks = db::checks_since(&conn, id, unix_now() - seconds)?;

    Ok(Json(StatsResponse {
        monitor_id: id,
        window,
        stats: stats::compute(&checks),
    }))
}
