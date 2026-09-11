CREATE TABLE project (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    id TEXT NOT NULL UNIQUE,
    operation_namespace TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK(format_version=1),
    metadata_version INTEGER NOT NULL DEFAULT 0 CHECK(metadata_version>=0)
) STRICT;
CREATE TABLE documents (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('chapter','note','character','world','theme','hook','scene')),
    title TEXT NOT NULL,
    metadata_version INTEGER NOT NULL DEFAULT 0 CHECK(metadata_version>=0),
    position INTEGER NOT NULL,
    working_version INTEGER NOT NULL DEFAULT 0 CHECK(working_version>=0),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    body_json TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    last_checkpoint_id TEXT,
    projection_dirty INTEGER NOT NULL DEFAULT 1 CHECK(projection_dirty IN (0,1)),
    trashed INTEGER NOT NULL DEFAULT 0 CHECK(trashed IN (0,1)),
    FOREIGN KEY(id,last_checkpoint_id) REFERENCES revisions(document_id,id) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE revisions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    source_working_version INTEGER NOT NULL CHECK(source_working_version>=0),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    body_json TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    parent_id TEXT,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(document_id,source_working_version),
    UNIQUE(document_id,id),
    FOREIGN KEY(document_id,parent_id) REFERENCES revisions(document_id,id)
) STRICT;
CREATE TABLE command_receipts (
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    payload_hash TEXT NOT NULL,
    operation_kind TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(operation_namespace,operation_id)
) STRICT;
CREATE TRIGGER revisions_no_update BEFORE UPDATE ON revisions BEGIN SELECT RAISE(ABORT,'Revisions are immutable'); END;
CREATE TRIGGER revisions_no_delete BEFORE DELETE ON revisions BEGIN SELECT RAISE(ABORT,'Revisions are immutable'); END;
CREATE TRIGGER receipts_no_update BEFORE UPDATE ON command_receipts BEGIN SELECT RAISE(ABORT,'Receipts are immutable'); END;
CREATE TRIGGER receipts_no_delete BEFORE DELETE ON command_receipts BEGIN SELECT RAISE(ABORT,'Receipts are immutable'); END;
