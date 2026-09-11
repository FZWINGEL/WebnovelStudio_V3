-- Explicit author instructions. These records are authored guidance, not
-- manuscript text or reviewed canon. A head points at the immutable current
-- version; historical versions remain available for snapshot inspection.
CREATE TABLE author_guidance_versions (
    version_id TEXT PRIMARY KEY,
    guidance_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version>=1),
    scope TEXT NOT NULL CHECK(scope IN ('request','document','project')),
    document_id TEXT REFERENCES documents(id),
    text TEXT NOT NULL CHECK(length(text)>0 AND length(CAST(text AS BLOB))<=16384),
    text_hash TEXT NOT NULL,
    active INTEGER NOT NULL CHECK(active IN (0,1)),
    origin_message_id TEXT REFERENCES discussion_messages(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(guidance_id,version)
) STRICT;

CREATE TABLE author_guidance_heads (
    guidance_id TEXT PRIMARY KEY,
    current_version_id TEXT NOT NULL UNIQUE REFERENCES author_guidance_versions(version_id),
    UNIQUE(guidance_id)
) STRICT;

-- Guidance operation receipts are deliberately separate from document save
-- receipts: a guidance mutation can apply to a project without a document.
-- The operation namespace is retained in recovered copies, so copied receipts
-- cannot authorize a new operation under the recovered namespace.
CREATE TABLE author_guidance_receipts (
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    operation_kind TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(operation_namespace,operation_id)
) STRICT;

-- Exact guidance records bound to an immutable story snapshot. The version
-- row is intentionally not deleted or rewritten when guidance is edited.
CREATE TABLE snapshot_guidance (
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    version_id TEXT NOT NULL REFERENCES author_guidance_versions(version_id),
    handle TEXT NOT NULL,
    text_hash TEXT NOT NULL,
    PRIMARY KEY(snapshot_id,handle),
    UNIQUE(snapshot_id,version_id)
) STRICT;

-- Request-scoped guidance is a one-use instruction. It is consumed only after
-- its snapshot and packet have successfully been compiled in the same owning
-- actor transaction.
CREATE TABLE guidance_request_uses (
    version_id TEXT PRIMARY KEY REFERENCES author_guidance_versions(version_id),
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id)
) STRICT;

CREATE TRIGGER author_guidance_versions_no_update BEFORE UPDATE ON author_guidance_versions
BEGIN SELECT RAISE(ABORT,'author guidance versions are immutable'); END;
CREATE TRIGGER author_guidance_versions_no_delete BEFORE DELETE ON author_guidance_versions
BEGIN SELECT RAISE(ABORT,'author guidance versions are immutable'); END;
CREATE TRIGGER author_guidance_receipts_no_update BEFORE UPDATE ON author_guidance_receipts
BEGIN SELECT RAISE(ABORT,'author guidance receipts are immutable'); END;
CREATE TRIGGER author_guidance_receipts_no_delete BEFORE DELETE ON author_guidance_receipts
BEGIN SELECT RAISE(ABORT,'author guidance receipts are immutable'); END;
CREATE TRIGGER snapshot_guidance_no_update BEFORE UPDATE ON snapshot_guidance
BEGIN SELECT RAISE(ABORT,'snapshot guidance pins are immutable'); END;
CREATE TRIGGER snapshot_guidance_no_delete BEFORE DELETE ON snapshot_guidance
BEGIN SELECT RAISE(ABORT,'snapshot guidance pins are immutable'); END;
CREATE TRIGGER guidance_request_uses_no_update BEFORE UPDATE ON guidance_request_uses
BEGIN SELECT RAISE(ABORT,'request guidance uses are immutable'); END;
CREATE TRIGGER guidance_request_uses_no_delete BEFORE DELETE ON guidance_request_uses
BEGIN SELECT RAISE(ABORT,'request guidance uses are immutable'); END;
