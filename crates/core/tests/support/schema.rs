use rusqlite::{Connection, params};

/// Strip schema-19-only columns from a current fixture before it is presented
/// as an older database. The production migration is intentionally one-way;
/// this helper only makes synthetic legacy fixtures truthful.
pub fn remove_schema19_features(connection: &Connection) -> rusqlite::Result<()> {
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
