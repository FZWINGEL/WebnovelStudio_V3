-- Story Workshop state is a small author-room projection. Documents and
-- immutable revisions remain the sole story authority; these rows retain the
-- exploration state, recoverable snapshots, and adoption boundaries.
CREATE TABLE IF NOT EXISTS workshop_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    version INTEGER NOT NULL CHECK(version>=0),
    state_json TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;

CREATE TABLE IF NOT EXISTS workshop_snapshots (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version>=0),
    payload_hash TEXT NOT NULL,
    state_json TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(operation_namespace,operation_id)
) STRICT;

CREATE TABLE IF NOT EXISTS workshop_adoption_previews (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    session_id TEXT NOT NULL,
    expected_version INTEGER NOT NULL CHECK(expected_version>=0),
    payload_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    preview_json TEXT NOT NULL,
    preview_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;

CREATE TABLE IF NOT EXISTS workshop_receipts (
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    operation_kind TEXT NOT NULL CHECK(operation_kind IN ('startWorkshop','saveWorkshop','adoptWorkshop')),
    payload_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(operation_namespace,operation_id)
) STRICT;

CREATE TRIGGER IF NOT EXISTS workshop_snapshots_no_update BEFORE UPDATE ON workshop_snapshots
BEGIN SELECT RAISE(ABORT,'Workshop snapshots are immutable'); END;
CREATE TRIGGER IF NOT EXISTS workshop_snapshots_no_delete BEFORE DELETE ON workshop_snapshots
BEGIN SELECT RAISE(ABORT,'Workshop snapshots are immutable'); END;
CREATE TRIGGER IF NOT EXISTS workshop_previews_no_update BEFORE UPDATE ON workshop_adoption_previews
BEGIN SELECT RAISE(ABORT,'Workshop adoption previews are immutable'); END;
CREATE TRIGGER IF NOT EXISTS workshop_previews_no_delete BEFORE DELETE ON workshop_adoption_previews
BEGIN SELECT RAISE(ABORT,'Workshop adoption previews are immutable'); END;
CREATE TRIGGER IF NOT EXISTS workshop_receipts_no_update BEFORE UPDATE ON workshop_receipts
BEGIN SELECT RAISE(ABORT,'Workshop receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS workshop_receipts_no_delete BEFORE DELETE ON workshop_receipts
BEGIN SELECT RAISE(ABORT,'Workshop receipts are immutable'); END;

-- Attach the reverse guard to the proposal table. It is dropped together with
-- that legacy table during synthetic schema downgrades, so the workshop
-- migration never leaves a trigger that prevents the downgrade from running.
CREATE TRIGGER IF NOT EXISTS proposal_receipts_no_workshop_collision
BEFORE INSERT ON proposal_receipts
WHEN EXISTS (
    SELECT 1 FROM workshop_receipts
    WHERE operation_namespace=NEW.operation_namespace
      AND operation_id=NEW.operation_id
)
BEGIN SELECT RAISE(ABORT,'Operation ID already belongs to a workshop command'); END;
