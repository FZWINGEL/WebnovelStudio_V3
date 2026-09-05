-- F1 staged V2 import evidence.  These rows are inert historical evidence;
-- they never become V3 canon, review decisions, or generation authority.
CREATE TABLE import_manifest (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    import_format_version INTEGER NOT NULL CHECK(import_format_version=1),
    source_project_id TEXT NOT NULL,
    source_schema_version INTEGER NOT NULL CHECK(source_schema_version=8),
    source_sha256 TEXT NOT NULL,
    source_bytes INTEGER NOT NULL CHECK(source_bytes>=0),
    source_title TEXT NOT NULL,
    source_slug TEXT NOT NULL,
    request_sha256 TEXT NOT NULL,
    counts_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE import_id_map (
    source_table TEXT NOT NULL,
    source_id TEXT NOT NULL,
    v3_document_id TEXT NOT NULL,
    source_project_id TEXT NOT NULL,
    PRIMARY KEY(source_table,source_id)
) STRICT;
CREATE TABLE import_body_decisions (
    source_chapter_id TEXT PRIMARY KEY NOT NULL,
    choice_kind TEXT NOT NULL CHECK(choice_kind IN ('working','empty','draft')),
    source_draft_id TEXT,
    source_working_state TEXT NOT NULL CHECK(source_working_state IN ('present','missing')),
    source_body_sha256 TEXT NOT NULL,
    selected_body_hash TEXT NOT NULL
) STRICT;
CREATE TABLE import_legacy_records (
    source_table TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_project_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY(source_table,source_id)
) STRICT;
