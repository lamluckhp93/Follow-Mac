//! SQLite access. A single connection is guarded by a mutex and every call is
//! executed on tokio's blocking pool so the async runtime / UI never stalls.

pub mod repository;

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Ordered list of migrations. Never edit an applied migration — append a new one.
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (1, "init", include_str!("migrations/0001_init.sql")),
    (2, "seed_defaults", include_str!("migrations/0002_seed_defaults.sql")),
];

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let db = Db { conn: Arc::new(Mutex::new(conn)), path: path.to_path_buf() };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        let db = Db { conn: Arc::new(Mutex::new(conn)), path: PathBuf::from(":memory:") };
        db.migrate()?;
        Ok(db)
    }

    fn configure(conn: &Connection) -> AppResult<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> AppResult<()> {
        let mut conn = self.lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );",
        )?;
        let current: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;
        for (version, name, sql) in MIGRATIONS {
            if *version <= current {
                continue;
            }
            let tx = conn.transaction()?;
            tx.execute_batch(sql)
                .map_err(|e| AppError::Database(format!("migration {version} ({name}) failed: {e}")))?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![version, name, crate::util::now_ms()],
            )?;
            tx.commit()?;
            tracing::info!(version, name, "applied database migration");
        }
        Ok(())
    }

    pub fn schema_version(&self) -> AppResult<i64> {
        let conn = self.lock()?;
        Ok(conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))?)
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| AppError::Database("database lock poisoned".into()))
    }

    /// Run a closure with the connection on the blocking thread pool.
    pub async fn call<F, R>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce(&mut Connection) -> AppResult<R> + Send + 'static,
        R: Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = db.lock()?;
            f(&mut conn)
        })
        .await?
    }

    /// Synchronous variant for use from non-async threads (sampler) and tests.
    pub fn call_sync<F, R>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce(&mut Connection) -> AppResult<R>,
    {
        let mut conn = self.lock()?;
        f(&mut conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        drop(db);
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        let rules: i64 = db
            .call_sync(|c| Ok(c.query_row("SELECT COUNT(*) FROM alert_rules", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(rules, 10);
    }
}
