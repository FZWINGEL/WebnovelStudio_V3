-- Immutable records of explicit whole-document draft exports.  The selected
-- revision is retained by the revisions table; this row records the exact
-- projection and installed basename without making the filesystem part of a
-- SQLite transaction.
CREATE TABLE export_records (
    id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    document_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    source_version INTEGER NOT NULL CHECK(source_version>=0),
    source_body_hash TEXT NOT NULL,
    working_draft INTEGER NOT NULL CHECK(working_draft IN (0,1)),
    format TEXT NOT NULL CHECK(format IN ('plainText','markdown')),
    format_version INTEGER NOT NULL CHECK(format_version=1),
    utf8_bytes INTEGER NOT NULL CHECK(utf8_bytes>=0),
    sha256 TEXT NOT NULL,
    basename TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(operation_namespace,id),
    FOREIGN KEY(document_id,revision_id) REFERENCES revisions(document_id,id)
) STRICT;

CREATE TRIGGER export_records_immutable_update
BEFORE UPDATE ON export_records
BEGIN SELECT RAISE(ABORT,'Export records are immutable'); END;
CREATE TRIGGER export_records_immutable_delete
BEFORE DELETE ON export_records
BEGIN SELECT RAISE(ABORT,'Export records are immutable'); END;
