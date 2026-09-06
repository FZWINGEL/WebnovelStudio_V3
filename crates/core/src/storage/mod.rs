use crate::projects::{CoreError, CoreResult};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

pub(crate) const LATEST_SCHEMA_VERSION: i64 = 20;

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
    if version < LATEST_SCHEMA_VERSION {
        if version > 0 {
            backup_before_upgrade(root, version)?;
        }
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if version < 1 {
            tx.execute_batch(include_str!("001_projects.sql"))?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("002_view_state.sql"))?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("003_story_context.sql"))?;
        }
        if version < 4 {
            tx.execute_batch(include_str!("004_context_packets.sql"))?;
        }
        if version < 5 {
            tx.execute_batch(include_str!("005_discussions.sql"))?;
        }
        if version < 6 {
            tx.execute_batch(include_str!("006_guidance.sql"))?;
        }
        if version < 7 {
            tx.execute_batch(include_str!("007_discussion_retry.sql"))?;
        }
        if version < 8 {
            tx.execute_batch(include_str!("008_proposals.sql"))?;
        }
        if version < 9 {
            tx.execute_batch(include_str!("009_exports.sql"))?;
        }
        if version < 10 {
            tx.execute_batch(include_str!("010_context_source_pins.sql"))?;
        }
        if version < 11 {
            tx.execute_batch(include_str!("011_safe_brief.sql"))?;
        }
        if version < 12 {
            tx.execute_batch(include_str!("012_v2_import.sql"))?;
        }
        if version < 13 {
            tx.execute_batch(include_str!("013_provider_results.sql"))?;
        }
        if version < 14 {
            tx.execute_batch(include_str!("014_reviewed_story.sql"))?;
        }
        if version < 15 {
            tx.execute_batch(include_str!("015_reviewed_context.sql"))?;
        }
        if version < 16 {
            tx.execute_batch(include_str!("016_memory.sql"))?;
        }
        if version < 17 {
            tx.execute_batch(include_str!("017_navigation_context.sql"))?;
        }
        if version < 19 {
            tx.execute_batch(include_str!("019_continuation.sql"))?;
        }
        if version < 20 {
            tx.execute_batch(include_str!("020_reviewed_export.sql"))?;
        }
        // Schema 18 changes no tables or historical bytes. It raises the reader
        // floor: schema-17 readers reject chapter-only navigation views from
        // an earlier source epoch, now valid under their exact dependencies.
        // The project version prevents those readers opening newer receipts.
        tx.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)?;
        tx.commit().map_err(CoreError::uncertain)?;
    }
    Ok(())
}

/// Take a durable, consistent pre-upgrade snapshot before changing an existing DB.
/// The backup remains beside the project so a failed migration can be
/// diagnosed or recovered without relying on the altered database.
fn backup_before_upgrade(root: &Path, version: i64) -> CoreResult<()> {
    let migrations = root.join("migrations");
    fs::create_dir_all(&migrations)?;
    let path = migrations.join(format!(
        "schema{version}-before-schema{LATEST_SCHEMA_VERSION}-{}.sqlite3",
        Uuid::new_v4()
    ));
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
