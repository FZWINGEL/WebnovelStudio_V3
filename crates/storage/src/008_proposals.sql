-- Candidates, prepared alternatives and decisions never replace a manuscript
-- until the author explicitly applies one through the document receipt path.
ALTER TABLE discussion_drafts ADD COLUMN intent TEXT NOT NULL DEFAULT 'discuss' CHECK(intent IN ('discuss','proposeEdits'));
CREATE TABLE proposals (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES discussion_runs(id),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 2),
    candidate_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(run_id, ordinal)
) STRICT;
CREATE TABLE proposal_versions (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    version INTEGER NOT NULL CHECK (version > 0),
    replacement_text TEXT NOT NULL,
    body_json TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(proposal_id, version)
) STRICT;
CREATE TABLE proposal_decisions (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL UNIQUE REFERENCES proposals(id),
    kind TEXT NOT NULL CHECK (kind IN ('apply','reject')),
    prepared_id TEXT REFERENCES proposal_versions(id),
    before_revision_id TEXT REFERENCES revisions(id),
    after_revision_id TEXT REFERENCES revisions(id),
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK ((kind='apply' AND prepared_id IS NOT NULL AND before_revision_id IS NOT NULL AND after_revision_id IS NOT NULL)
        OR (kind='reject' AND prepared_id IS NULL AND before_revision_id IS NULL AND after_revision_id IS NULL)),
    UNIQUE(operation_namespace, operation_id)
) STRICT;
CREATE TABLE proposal_receipts (
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('prepare','reject')),
    payload_hash TEXT NOT NULL,
    result_id TEXT NOT NULL,
    PRIMARY KEY(operation_namespace, operation_id)
) STRICT;
CREATE TRIGGER proposals_immutable_update BEFORE UPDATE ON proposals BEGIN SELECT RAISE(ABORT,'Immutable proposal'); END;
CREATE TRIGGER proposals_immutable_delete BEFORE DELETE ON proposals BEGIN SELECT RAISE(ABORT,'Immutable proposal'); END;
CREATE TRIGGER proposal_versions_immutable_update BEFORE UPDATE ON proposal_versions BEGIN SELECT RAISE(ABORT,'Immutable prepared proposal'); END;
CREATE TRIGGER proposal_versions_immutable_delete BEFORE DELETE ON proposal_versions BEGIN SELECT RAISE(ABORT,'Immutable prepared proposal'); END;
CREATE TRIGGER proposal_decisions_immutable_update BEFORE UPDATE ON proposal_decisions BEGIN SELECT RAISE(ABORT,'Immutable author decision'); END;
CREATE TRIGGER proposal_decisions_immutable_delete BEFORE DELETE ON proposal_decisions BEGIN SELECT RAISE(ABORT,'Immutable author decision'); END;
CREATE TRIGGER proposal_receipts_immutable_update BEFORE UPDATE ON proposal_receipts BEGIN SELECT RAISE(ABORT,'Immutable proposal receipt'); END;
CREATE TRIGGER proposal_receipts_immutable_delete BEFORE DELETE ON proposal_receipts BEGIN SELECT RAISE(ABORT,'Immutable proposal receipt'); END;
-- Command and proposal receipts share the caller's operation namespace. Keep
-- an operation identity in one domain so a retry cannot cross a command kind.
CREATE TRIGGER proposal_receipts_no_command_collision
BEFORE INSERT ON proposal_receipts
WHEN EXISTS (
    SELECT 1 FROM command_receipts
    WHERE operation_namespace=NEW.operation_namespace
      AND operation_id=NEW.operation_id
)
BEGIN
    SELECT RAISE(ABORT,'Operation ID already belongs to a document command');
END;
CREATE TRIGGER command_receipts_no_proposal_collision
BEFORE INSERT ON command_receipts
WHEN EXISTS (
    SELECT 1 FROM proposal_receipts
    WHERE operation_namespace=NEW.operation_namespace
      AND operation_id=NEW.operation_id
)
BEGIN
    SELECT RAISE(ABORT,'Operation ID already belongs to a proposal command');
END;
