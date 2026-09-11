-- Author-only reviewed prose basis.  These records pin exact revisions and
-- selected chapter order; they do not contain generated canon or summaries.
CREATE TABLE review_stages (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    document_id TEXT NOT NULL,
    target_version INTEGER NOT NULL CHECK(target_version>=0),
    target_body_hash TEXT NOT NULL,
    target_revision_id TEXT NOT NULL,
    source_epoch INTEGER NOT NULL CHECK(source_epoch>=0),
    policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
    previous_bundle_id TEXT,
    prefix_json TEXT NOT NULL,
    prefix_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(operation_namespace,operation_id),
    FOREIGN KEY(document_id,target_revision_id) REFERENCES revisions(document_id,id)
) STRICT;

CREATE TABLE ready_bundles (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    stage_id TEXT NOT NULL REFERENCES review_stages(id),
    document_id TEXT NOT NULL,
    target_version INTEGER NOT NULL CHECK(target_version>=0),
    target_body_hash TEXT NOT NULL,
    target_revision_id TEXT NOT NULL,
    source_epoch INTEGER NOT NULL CHECK(source_epoch>=0),
    policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
    previous_bundle_id TEXT,
    prefix_json TEXT NOT NULL,
    prefix_hash TEXT NOT NULL,
    coverage TEXT NOT NULL CHECK(coverage='authorOnly'),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(operation_namespace,operation_id),
    FOREIGN KEY(document_id,target_revision_id) REFERENCES revisions(document_id,id)
) STRICT;

-- There is one mutable selected head per chapter in the current operation
-- namespace. Historical bundles intentionally remain outside this table.
CREATE TABLE ready_heads (
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    document_id TEXT NOT NULL,
    bundle_id TEXT NOT NULL REFERENCES ready_bundles(id),
    PRIMARY KEY(project_id,operation_namespace,document_id)
) STRICT;

-- A later selected bundle is explicitly fenced when an earlier chapter is
-- superseded. Prefix validation remains authoritative; this table makes the
-- known dependency visible without introducing a general event log.
CREATE TABLE review_fences (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    affected_bundle_id TEXT NOT NULL REFERENCES ready_bundles(id),
    changed_document_id TEXT NOT NULL,
    changed_version INTEGER NOT NULL CHECK(changed_version>=0),
    changed_body_hash TEXT NOT NULL,
    superseding_bundle_id TEXT NOT NULL REFERENCES ready_bundles(id),
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;

CREATE TRIGGER review_stages_no_update BEFORE UPDATE ON review_stages
BEGIN SELECT RAISE(ABORT,'Review stages are immutable'); END;
CREATE TRIGGER review_stages_no_delete BEFORE DELETE ON review_stages
BEGIN SELECT RAISE(ABORT,'Review stages are immutable'); END;
CREATE TRIGGER ready_bundles_no_update BEFORE UPDATE ON ready_bundles
BEGIN SELECT RAISE(ABORT,'Ready bundles are immutable'); END;
CREATE TRIGGER ready_bundles_no_delete BEFORE DELETE ON ready_bundles
BEGIN SELECT RAISE(ABORT,'Ready bundles are immutable'); END;
CREATE TRIGGER review_fences_no_update BEFORE UPDATE ON review_fences
BEGIN SELECT RAISE(ABORT,'Review fences are immutable'); END;
CREATE TRIGGER review_fences_no_delete BEFORE DELETE ON review_fences
BEGIN SELECT RAISE(ABORT,'Review fences are immutable'); END;
