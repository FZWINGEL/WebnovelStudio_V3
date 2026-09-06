#![allow(dead_code)]

use rusqlite::{Connection, params};

/// Strip schema-24, schema-21, schema-20, and schema-19-only columns from a current fixture before it
/// is presented as an older database. The production migrations are
/// intentionally one-way; this helper only makes synthetic legacy fixtures
/// truthful.
pub fn remove_schema19_features(connection: &Connection) -> rusqlite::Result<()> {
    remove_schema22_features(connection)?;
    remove_schema20_features(connection)?;
    drop_column_if_present(connection, "export_records", "review_bundle_id")?;
    drop_column_if_present(connection, "proposals", "kind")?;
    drop_column_if_present(connection, "proposal_versions", "payload_json")?;

    if table_has_column(connection, "discussion_drafts", "basis")? {
        // SQLite cannot drop `basis` while the schema-19 cross-field CHECK
        // still refers to it. Rebuild the pre-19 shape and copy every stored
        // value verbatim before older fixture SQL removes any earlier column.
        connection.execute_batch(
            "ALTER TABLE discussion_drafts RENAME TO discussion_drafts_schema19;
             CREATE TABLE discussion_drafts (
                 project_id TEXT NOT NULL,
                 operation_namespace TEXT NOT NULL,
                 document_id TEXT NOT NULL REFERENCES documents(id),
                 version INTEGER NOT NULL CHECK(version>=0),
                 text TEXT NOT NULL,
                 scope_json TEXT,
                 pinned_document_ids_json TEXT NOT NULL,
                 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                 previous_run_id TEXT REFERENCES discussion_runs(id),
                 safe_brief_json TEXT,
                 intent TEXT NOT NULL DEFAULT 'discuss'
                     CHECK(intent IN ('discuss','proposeEdits')),
                 PRIMARY KEY(project_id,operation_namespace,document_id)
             ) STRICT;
             INSERT INTO discussion_drafts(
                 project_id,operation_namespace,document_id,version,text,scope_json,
                 pinned_document_ids_json,updated_at,previous_run_id,safe_brief_json,intent
             )
             SELECT
                 project_id,operation_namespace,document_id,version,text,scope_json,
                 pinned_document_ids_json,updated_at,previous_run_id,safe_brief_json,intent
             FROM discussion_drafts_schema19;
             DROP TABLE discussion_drafts_schema19;",
        )?;
    }
    Ok(())
}

/// Strip the schema-24 bounded story-lookup records from a current fixture.
///
/// The lookup tables are only present in the current schema, so legacy fixture
/// builders must remove them before lowering `user_version`. Their foreign-key
/// graph is intentionally dismantled from leaves to root, and their immutable
/// triggers are removed before their tables. This helper is test-only; the
/// production migration remains a one-way upgrade.
pub fn remove_schema24_features(connection: &Connection) -> rusqlite::Result<()> {
    for trigger in [
        "discussion_lookup_results_no_update",
        "discussion_lookup_results_no_delete",
        "discussion_lookup_reads_no_update",
        "discussion_lookup_reads_no_delete",
    ] {
        drop_trigger_if_present(connection, trigger)?;
    }

    // Reads and results reference invocations, so remove the child tables
    // first even when a fixture happens to contain lookup rows.
    for table in [
        "discussion_lookup_reads",
        "discussion_lookup_results",
        "discussion_lookup_invocations",
    ] {
        drop_table_if_present(connection, table)?;
    }

    drop_column_if_present(connection, "discussion_drafts", "lookup_json")?;
    Ok(())
}

/// Strip only schema-23 promise columns so a current fixture can truthfully be
/// presented as a schema-22 database before migration coverage runs.
pub fn remove_schema22_features(connection: &Connection) -> rusqlite::Result<()> {
    drop_column_if_present(connection, "review_stages", "promises_json")?;
    drop_column_if_present(connection, "review_stages", "promises_hash")?;
    drop_column_if_present(connection, "ready_bundles", "promises_json")?;
    drop_column_if_present(connection, "ready_bundles", "promises_hash")?;
    Ok(())
}

/// Strip only schema-21 reviewed evidence columns so a truthful schema-20
/// archive can exercise the next migration without removing older features.
pub fn remove_schema20_features(connection: &Connection) -> rusqlite::Result<()> {
    drop_column_if_present(connection, "review_stages", "records_json")?;
    drop_column_if_present(connection, "review_stages", "records_hash")?;
    drop_column_if_present(connection, "ready_bundles", "records_json")?;
    drop_column_if_present(connection, "ready_bundles", "records_hash")?;
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?) WHERE name=?",
        params![table, column],
        |row| row.get(0),
    )?;
    Ok(count != 0)
}

fn drop_column_if_present(
    connection: &Connection,
    table: &str,
    column: &str,
) -> rusqlite::Result<()> {
    if table_has_column(connection, table, column)? {
        let sql = format!("ALTER TABLE {table} DROP COLUMN {column}");
        connection.execute_batch(&sql)?;
    }
    Ok(())
}

fn drop_table_if_present(connection: &Connection, table: &str) -> rusqlite::Result<()> {
    let exists: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        [table],
        |row| row.get(0),
    )?;
    if exists != 0 {
        let sql = format!("DROP TABLE {table}");
        connection.execute_batch(&sql)?;
    }
    Ok(())
}

fn drop_trigger_if_present(connection: &Connection, trigger: &str) -> rusqlite::Result<()> {
    let exists: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
        [trigger],
        |row| row.get(0),
    )?;
    if exists != 0 {
        let sql = format!("DROP TRIGGER {trigger}");
        connection.execute_batch(&sql)?;
    }
    Ok(())
}
