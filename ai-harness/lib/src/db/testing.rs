//! Test-only helpers for exercising the real file-open path (`Db::open`,
//! WAL, `busy_timeout`) rather than [`super::Db::in_memory`]. Mirrors the
//! `TempDir` pattern in `crate::cache`'s test module.

use std::path::PathBuf;

use crate::config::DatabaseConfig;

use super::Db;

pub(crate) struct TempDb {
    dir: PathBuf,
    pub db: Db,
}

impl TempDb {
    pub(crate) async fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ai-harness-db-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let config = DatabaseConfig {
            path: dir.join("ai-harness.db"),
        };
        let db = Db::open(&config).await.expect("opening a fresh temp database");
        Self { dir, db }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_creates_the_parent_directory_and_migrates() {
        let temp = TempDb::new("open").await;
        assert!(temp.dir.join("ai-harness.db").exists());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_configs")
            .fetch_one(temp.db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn reopening_the_same_file_does_not_reapply_migrations() {
        let temp = TempDb::new("reopen").await;
        let config = DatabaseConfig {
            path: temp.dir.join("ai-harness.db"),
        };
        // A second open against the same file must not error or re-run
        // migration 1.
        let db2 = Db::open(&config).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _migrations")
            .fetch_one(db2.pool())
            .await
            .unwrap();
        assert_eq!(count, super::super::migrate::MIGRATIONS.len() as i64);
    }
}
