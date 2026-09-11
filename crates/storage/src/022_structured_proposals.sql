-- Structured block proposals extend the durable discriminator while retaining
-- every legacy passage/continuation byte and immutable proposal identity.
-- SQLite cannot alter the existing CHECK constraint in place, so rebuild the
-- three proposal tables together. Child rows are copied before their old
-- tables are dropped, and all durable IDs/payloads remain unchanged.
ALTER TABLE proposals RENAME TO proposals_before_structured;
ALTER TABLE proposal_versions RENAME TO proposal_versions_before_structured;
ALTER TABLE proposal_decisions RENAME TO proposal_decisions_before_structured;

CREATE TABLE proposals (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES discussion_runs(id),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 2),
    candidate_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    kind TEXT NOT NULL DEFAULT 'passage'
        CHECK (kind IN ('passage','continuation','structured')),
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
    payload_json TEXT,
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

INSERT INTO proposals(id,run_id,ordinal,candidate_json,created_at,kind)
SELECT id,run_id,ordinal,candidate_json,created_at,kind
FROM proposals_before_structured;

INSERT INTO proposal_versions(id,proposal_id,version,replacement_text,body_json,body_hash,created_at,payload_json)
SELECT id,proposal_id,version,replacement_text,body_json,body_hash,created_at,payload_json
FROM proposal_versions_before_structured;

INSERT INTO proposal_decisions(id,proposal_id,kind,prepared_id,before_revision_id,after_revision_id,operation_namespace,operation_id,created_at)
SELECT id,proposal_id,kind,prepared_id,before_revision_id,after_revision_id,operation_namespace,operation_id,created_at
FROM proposal_decisions_before_structured;

DROP TABLE proposal_decisions_before_structured;
DROP TABLE proposal_versions_before_structured;
DROP TABLE proposals_before_structured;

CREATE TRIGGER proposals_immutable_update BEFORE UPDATE ON proposals BEGIN SELECT RAISE(ABORT,'Immutable proposal'); END;
CREATE TRIGGER proposals_immutable_delete BEFORE DELETE ON proposals BEGIN SELECT RAISE(ABORT,'Immutable proposal'); END;
CREATE TRIGGER proposal_versions_immutable_update BEFORE UPDATE ON proposal_versions BEGIN SELECT RAISE(ABORT,'Immutable prepared proposal'); END;
CREATE TRIGGER proposal_versions_immutable_delete BEFORE DELETE ON proposal_versions BEGIN SELECT RAISE(ABORT,'Immutable prepared proposal'); END;
CREATE TRIGGER proposal_decisions_immutable_update BEFORE UPDATE ON proposal_decisions BEGIN SELECT RAISE(ABORT,'Immutable author decision'); END;
CREATE TRIGGER proposal_decisions_immutable_delete BEFORE DELETE ON proposal_decisions BEGIN SELECT RAISE(ABORT,'Immutable author decision'); END;
