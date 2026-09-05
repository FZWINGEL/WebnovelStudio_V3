use crate::projects::{CoreError, CoreResult};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

const LATEST_SCHEMA_VERSION: i64 = 2;

pub(crate) fn configure(connection: &Connection) -> CoreResult<()> {
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
    )?;
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    let sync: i64 = connection.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    let keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    if mode != "wal" || sync != 2 || keys != 1 {
        return Err(CoreError::new(
            "PersistenceUnavailable",
            "Required SQLite durability settings are unavailable.",
        ));
    }
    Ok(())
}

pub(crate) fn migrate(connection: &mut Connection, root: &Path) -> CoreResult<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > LATEST_SCHEMA_VERSION {
        return Err(CoreError::new(
            "UnsupportedSchema",
            "This project needs a newer version of WebnovelStudio.",
        ));
    }
    if version == 0 {
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch(include_str!("001_projects.sql"))?;
        tx.execute_batch(include_str!("002_view_state.sql"))?;
        tx.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)?;
        tx.commit().map_err(CoreError::uncertain)?;
    } else if version == 1 {
        backup_before_schema2(root)?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch(include_str!("002_view_state.sql"))?;
        tx.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)?;
        tx.commit().map_err(CoreError::uncertain)?;
    }
    Ok(())
}

/// Take a durable, consistent pre-upgrade snapshot before changing a v1 DB.
/// The backup remains beside the project so a failed migration can be
/// diagnosed or recovered without relying on the altered database.
fn backup_before_schema2(root: &Path) -> CoreResult<()> {
    let migrations = root.join("migrations");
    fs::create_dir_all(&migrations)?;
    let path = migrations.join(format!("schema1-before-schema2-{}.sqlite3", Uuid::new_v4()));
    let db_path = root.join("project.sqlite3");
    let source = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let placeholder = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    drop(placeholder);
    let mut destination = Connection::open(&path)?;
    Backup::new(&source, &mut destination)?.run_to_completion(
        64,
        Duration::from_millis(10),
        None,
    )?;
    drop(destination);
    let backup = OpenOptions::new().read(true).write(true).open(&path)?;
    backup.sync_all()?;
    Ok(())
}
