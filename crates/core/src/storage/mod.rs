use crate::projects::{CoreError, CoreResult};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

pub(crate) const LATEST_SCHEMA_VERSION: i64 = 33;

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
        if version < 21 {
            tx.execute_batch(include_str!("021_reviewed_story_evidence.sql"))?;
        }
        if version < 22 {
            tx.execute_batch(include_str!("022_structured_proposals.sql"))?;
        }
        if version < 23 {
            tx.execute_batch(include_str!("023_reviewed_promises.sql"))?;
        }
        if version < 24 {
            tx.execute_batch(include_str!("024_discussion_lookup.sql"))?;
        }
        if version < 26 {
            tx.execute_batch(include_str!("026_http_provider_delivery.sql"))?;
        }
        if version < 29 {
            tx.execute_batch(include_str!("029_memory_provider_delivery.sql"))?;
        }
        if version < 30 {
            tx.execute_batch(include_str!("030_claude_provider_result.sql"))?;
        }
        // Schema 31 changes no tables and exists as a reader floor for
        // author-room structured document-development packets. Older readers
        // must reject the project before reopening packets they cannot
        // validate.
        if version < 31 {
            tx.execute_batch("SELECT 1;")?;
        }
        if version < 32 {
            let columns = [
                ("review_stages", "summary_json"),
                ("review_stages", "summary_hash"),
                ("ready_bundles", "summary_json"),
                ("ready_bundles", "summary_hash"),
            ];
            let mut missing = Vec::new();
            for &(table, column) in &columns {
                let present: bool = tx.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"
                    ),
                    [column],
                    |row| row.get(0),
                )?;
                if !present {
                    missing.push((table, column));
                }
            }
            if missing.len() == columns.len() {
                tx.execute_batch(include_str!("032_reviewed_summaries.sql"))?;
            } else {
                for (table, column) in missing {
                    tx.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
                }
            }
        }
        if version < 33 {
            let columns = [
                ("review_stages", "knowledge_json"),
                ("review_stages", "knowledge_hash"),
                ("ready_bundles", "knowledge_json"),
                ("ready_bundles", "knowledge_hash"),
            ];
            let mut missing = Vec::new();
            for &(table, column) in &columns {
                let present: bool = tx.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"
                    ),
                    [column],
                    |row| row.get(0),
                )?;
                if !present {
                    missing.push((table, column));
                }
            }
            if missing.len() == columns.len() {
                tx.execute_batch(include_str!("033_reviewed_knowledge.sql"))?;
            } else {
                for (table, column) in missing {
                    tx.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
                }
            }
        }
        // Schema 30 adds the nullable Claude terminal model claim. NULL keeps
        // historical Codex and HTTP receipts byte-compatible while the reader
        // floor prevents older applications from reopening Claude receipts.
        // Schema 28 changes no tables: new lookup packets include a versioned
        // source-title projection that older readers cannot validate. Absent
        // projections in historical packets keep their exact serialized bytes.
        // Schema 27 changes no tables: author-selected Codex bindings have a
        // distinct profile that older readers cannot validate. Exact earlier
        // packet bytes and the fixed maintenance profile remain unchanged.
        // Schema 25 also changes no tables: new provider bindings record the
        // app profile and observed CLI identity independently. Older readers
        // cannot validate those receipts. Historical packet bytes stay intact.
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
