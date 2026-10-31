use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub expected_status: u16,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub monitor_id: i64,
    /// Unix timestamp in seconds.
    pub checked_at: i64,
    pub status_code: Option<u16>,
    pub response_ms: Option<i64>,
    pub up: bool,
    pub error: Option<String>,
}

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS monitors (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            expected_status INTEGER NOT NULL DEFAULT 200,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS checks (
            id INTEGER PRIMARY KEY,
            monitor_id INTEGER NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
            checked_at INTEGER NOT NULL,
            status_code INTEGER,
            response_ms INTEGER,
            up INTEGER NOT NULL,
            error TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_checks_monitor_time ON checks(monitor_id, checked_at);",
    )
}

fn monitor_from_row(row: &Row) -> rusqlite::Result<Monitor> {
    Ok(Monitor {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        expected_status: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn check_from_row(row: &Row) -> rusqlite::Result<Check> {
    Ok(Check {
        monitor_id: row.get(0)?,
        checked_at: row.get(1)?,
        status_code: row.get(2)?,
        response_ms: row.get(3)?,
        up: row.get(4)?,
        error: row.get(5)?,
    })
}

pub fn list_monitors(conn: &Connection) -> rusqlite::Result<Vec<Monitor>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, url, expected_status, created_at FROM monitors ORDER BY id",
    )?;
    let rows = stmt.query_map([], monitor_from_row)?;
    rows.collect()
}

pub fn get_monitor(conn: &Connection, id: i64) -> rusqlite::Result<Option<Monitor>> {
    conn.query_row(
        "SELECT id, name, url, expected_status, created_at FROM monitors WHERE id = ?1",
        params![id],
        monitor_from_row,
    )
    .optional()
}

pub fn insert_monitor(
    conn: &Connection,
    name: &str,
    url: &str,
    expected_status: u16,
    now: i64,
) -> rusqlite::Result<Monitor> {
    conn.execute(
        "INSERT INTO monitors (name, url, expected_status, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![name, url, expected_status, now],
    )?;
    Ok(Monitor {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        url: url.to_string(),
        expected_status,
        created_at: now,
    })
}

pub fn delete_monitor(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    let deleted = conn.execute("DELETE FROM monitors WHERE id = ?1", params![id])?;
    Ok(deleted > 0)
}

pub fn insert_check(conn: &Connection, check: &Check) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO checks (monitor_id, checked_at, status_code, response_ms, up, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            check.monitor_id,
            check.checked_at,
            check.status_code,
            check.response_ms,
            check.up,
            check.error,
        ],
    )?;
    Ok(())
}

/// Checks for one monitor at or after `since`, oldest first.
pub fn checks_since(conn: &Connection, monitor_id: i64, since: i64) -> rusqlite::Result<Vec<Check>> {
    let mut stmt = conn.prepare(
        "SELECT monitor_id, checked_at, status_code, response_ms, up, error
         FROM checks WHERE monitor_id = ?1 AND checked_at >= ?2 ORDER BY checked_at",
    )?;
    let rows = stmt.query_map(params![monitor_id, since], check_from_row)?;
    rows.collect()
}

pub fn last_check(conn: &Connection, monitor_id: i64) -> rusqlite::Result<Option<Check>> {
    conn.query_row(
        "SELECT monitor_id, checked_at, status_code, response_ms, up, error
         FROM checks WHERE monitor_id = ?1 ORDER BY checked_at DESC LIMIT 1",
        params![monitor_id],
        check_from_row,
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&conn).unwrap();
        conn
    }

    fn check_at(monitor_id: i64, checked_at: i64, up: bool) -> Check {
        Check {
            monitor_id,
            checked_at,
            status_code: Some(if up { 200 } else { 503 }),
            response_ms: Some(120),
            up,
            error: None,
        }
    }

    #[test]
    fn insert_and_get_monitor() {
        let conn = test_conn();
        let created = insert_monitor(&conn, "api", "https://example.com", 200, 1_000).unwrap();
        let fetched = get_monitor(&conn, created.id).unwrap().unwrap();
        assert_eq!(fetched.name, "api");
        assert_eq!(fetched.url, "https://example.com");
    }

    #[test]
    fn checks_since_filters_by_time_and_monitor() {
        let conn = test_conn();
        let a = insert_monitor(&conn, "a", "https://a.example", 200, 0).unwrap();
        let b = insert_monitor(&conn, "b", "https://b.example", 200, 0).unwrap();
        for (monitor, at) in [(a.id, 100), (a.id, 200), (a.id, 300), (b.id, 250)] {
            insert_check(&conn, &check_at(monitor, at, true)).unwrap();
        }

        let checks = checks_since(&conn, a.id, 200).unwrap();
        let times: Vec<i64> = checks.iter().map(|c| c.checked_at).collect();
        assert_eq!(times, vec![200, 300]);
    }

    #[test]
    fn last_check_returns_newest() {
        let conn = test_conn();
        let m = insert_monitor(&conn, "m", "https://m.example", 200, 0).unwrap();
        insert_check(&conn, &check_at(m.id, 100, true)).unwrap();
        insert_check(&conn, &check_at(m.id, 200, false)).unwrap();

        let last = last_check(&conn, m.id).unwrap().unwrap();
        assert_eq!(last.checked_at, 200);
        assert!(!last.up);
    }

    #[test]
    fn deleting_monitor_removes_its_checks() {
        let conn = test_conn();
        let m = insert_monitor(&conn, "m", "https://m.example", 200, 0).unwrap();
        insert_check(&conn, &check_at(m.id, 100, true)).unwrap();

        assert!(delete_monitor(&conn, m.id).unwrap());
        assert!(checks_since(&conn, m.id, 0).unwrap().is_empty());
    }
}
