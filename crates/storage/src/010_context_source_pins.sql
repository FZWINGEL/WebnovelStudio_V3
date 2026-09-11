-- Persistent AuthorRoom source preferences.  Project scope uses an empty
-- target key so the composite key remains non-null and deterministic in
-- SQLite; the public DTO maps it back to null.
CREATE TABLE source_pin_sets (
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('project','document')),
    target_document_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version>=0),
    source_document_ids_json TEXT NOT NULL,
    audience TEXT NOT NULL CHECK(audience='authorRoom'),
    PRIMARY KEY(project_id,operation_namespace,scope,target_document_id),
    CHECK((scope='project' AND target_document_id='') OR
          (scope='document' AND length(target_document_id)>0))
    -- Empty project target has no corresponding document row; owner code
    -- validates document targets before writing.
) STRICT;

CREATE TABLE source_pin_receipts (
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('project','document')),
    target_document_id TEXT NOT NULL,
    expected_version INTEGER NOT NULL CHECK(expected_version>=0),
    payload_hash TEXT NOT NULL,
    operation_kind TEXT NOT NULL CHECK(operation_kind='saveSourcePins'),
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(operation_namespace,operation_id)
    -- Empty project target has no corresponding document row; owner code
    -- validates document targets before writing.
) STRICT;

CREATE TRIGGER source_pin_receipts_no_update
BEFORE UPDATE ON source_pin_receipts BEGIN SELECT RAISE(ABORT,'Source pin receipts are immutable'); END;
CREATE TRIGGER source_pin_receipts_no_delete
BEFORE DELETE ON source_pin_receipts BEGIN SELECT RAISE(ABORT,'Source pin receipts are immutable'); END;
