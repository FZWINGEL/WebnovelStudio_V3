ALTER TABLE project
    ADD COLUMN disclosure_policy_epoch INTEGER NOT NULL DEFAULT 0
    CHECK(disclosure_policy_epoch>=0);

-- Historical project identity deliberately remains on a copied snapshot. A
-- recovered project cannot use the original project's request authorization.
CREATE TABLE story_snapshots (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    context_source_epoch INTEGER NOT NULL CHECK(context_source_epoch>=0),
    disclosure_policy_epoch INTEGER NOT NULL CHECK(disclosure_policy_epoch>=0),
    manifest_json TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(operation_namespace,operation_id)
) STRICT;

CREATE TABLE snapshot_sources (
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    handle TEXT NOT NULL,
    document_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    PRIMARY KEY(snapshot_id,handle),
    UNIQUE(snapshot_id,document_id,revision_id),
    FOREIGN KEY(document_id,revision_id) REFERENCES revisions(document_id,id)
) STRICT;

CREATE TRIGGER immutable_story_snapshot_update BEFORE UPDATE ON story_snapshots
BEGIN SELECT RAISE(ABORT,'story snapshots are immutable'); END;
CREATE TRIGGER immutable_story_snapshot_delete BEFORE DELETE ON story_snapshots
BEGIN SELECT RAISE(ABORT,'story snapshots are immutable'); END;
CREATE TRIGGER immutable_snapshot_source_update BEFORE UPDATE ON snapshot_sources
BEGIN SELECT RAISE(ABORT,'snapshot sources are immutable'); END;
CREATE TRIGGER immutable_snapshot_source_delete BEFORE DELETE ON snapshot_sources
BEGIN SELECT RAISE(ABORT,'snapshot sources are immutable'); END;

-- Disposable search material. Only immutable revisions own the source text.
CREATE TABLE passage_projections (
    revision_id TEXT NOT NULL REFERENCES revisions(id),
    projection_version INTEGER NOT NULL DEFAULT 1 CHECK(projection_version>0),
    block_id TEXT NOT NULL,
    block_order INTEGER NOT NULL CHECK(block_order>=0),
    body_hash TEXT NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY(revision_id,block_id)
) STRICT;

CREATE TABLE document_aliases (
    document_id TEXT NOT NULL REFERENCES documents(id),
    alias TEXT NOT NULL CHECK(length(alias)>0),
    PRIMARY KEY(document_id,alias)
) STRICT;
