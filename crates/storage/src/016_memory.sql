-- Source-bound generated navigation memory.  These rows are aids only: they
-- never replace a manuscript revision or an accepted story record.
CREATE TABLE memory_jobs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    target_document_id TEXT NOT NULL REFERENCES documents(id),
    target_version INTEGER NOT NULL CHECK(target_version>=0),
    target_body_hash TEXT NOT NULL,
    source_document_id TEXT NOT NULL,
    source_revision_id TEXT NOT NULL,
    source_body_hash TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    context_source_epoch INTEGER NOT NULL CHECK(context_source_epoch>=0),
    disclosure_policy_epoch INTEGER NOT NULL CHECK(disclosure_policy_epoch>=0),
    status TEXT NOT NULL CHECK(status IN ('queued','running','stopping','completed','stopped','failed','interrupted')),
    dispatch_state TEXT NOT NULL CHECK(dispatch_state IN ('pending','dispatched')),
    stop_reason TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(operation_namespace,operation_id),
    FOREIGN KEY(source_document_id,source_revision_id) REFERENCES revisions(document_id,id),
    CHECK(source_document_id=target_document_id)
) STRICT;

-- Lifecycle is mutable on purpose.  The request identity and frozen basis
-- are fenced by the trigger below; dispatch/recovery only changes lifecycle.
CREATE TRIGGER memory_jobs_identity_no_update BEFORE UPDATE ON memory_jobs
WHEN OLD.id<>NEW.id
  OR OLD.project_id<>NEW.project_id
  OR OLD.operation_namespace<>NEW.operation_namespace
  OR OLD.operation_id<>NEW.operation_id
  OR OLD.payload_hash<>NEW.payload_hash
  OR OLD.request_json<>NEW.request_json
  OR OLD.target_document_id<>NEW.target_document_id
  OR OLD.target_version<>NEW.target_version
  OR OLD.target_body_hash<>NEW.target_body_hash
  OR OLD.source_document_id<>NEW.source_document_id
  OR OLD.source_revision_id<>NEW.source_revision_id
  OR OLD.source_body_hash<>NEW.source_body_hash
  OR OLD.snapshot_id<>NEW.snapshot_id
  OR OLD.packet_id<>NEW.packet_id
  OR OLD.context_source_epoch<>NEW.context_source_epoch
  OR OLD.disclosure_policy_epoch<>NEW.disclosure_policy_epoch
  OR OLD.created_at<>NEW.created_at
BEGIN SELECT RAISE(ABORT,'memory job request identity is immutable'); END;

CREATE TABLE memory_results (
    job_id TEXT PRIMARY KEY REFERENCES memory_jobs(id),
    event_id TEXT NOT NULL,
    raw_output TEXT NOT NULL,
    raw_output_hash TEXT NOT NULL,
    candidate_json TEXT,
    outcome TEXT NOT NULL CHECK(outcome IN ('completed','stopped','timed_out','output_limit','failed')),
    confirmed_stdin_bytes INTEGER CHECK(confirmed_stdin_bytes IS NULL OR confirmed_stdin_bytes>=0),
    usage_json TEXT,
    cleanup TEXT CHECK(cleanup IS NULL OR cleanup IN ('settled','unresolved')),
    error TEXT,
    validation_error TEXT,
    effective_identity TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(job_id,event_id)
) STRICT;

CREATE TRIGGER memory_results_no_update BEFORE UPDATE ON memory_results
BEGIN SELECT RAISE(ABORT,'memory terminal results are immutable'); END;
CREATE TRIGGER memory_results_no_delete BEFORE DELETE ON memory_results
BEGIN SELECT RAISE(ABORT,'memory terminal results are immutable'); END;

CREATE TABLE memory_views (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL UNIQUE REFERENCES memory_jobs(id),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    target_version INTEGER NOT NULL CHECK(target_version>=0),
    target_body_hash TEXT NOT NULL,
    source_revision_id TEXT NOT NULL,
    source_body_hash TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    context_source_epoch INTEGER NOT NULL CHECK(context_source_epoch>=0),
    disclosure_policy_epoch INTEGER NOT NULL CHECK(disclosure_policy_epoch>=0),
    candidate_json TEXT NOT NULL,
    installed_current INTEGER NOT NULL CHECK(installed_current IN (0,1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    FOREIGN KEY(document_id,source_revision_id) REFERENCES revisions(document_id,id)
) STRICT;

CREATE TABLE memory_view_sources (
    view_id TEXT NOT NULL REFERENCES memory_views(id),
    document_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    PRIMARY KEY(view_id,document_id,revision_id),
    FOREIGN KEY(document_id,revision_id) REFERENCES revisions(document_id,id)
) STRICT;

CREATE TRIGGER memory_views_no_update BEFORE UPDATE ON memory_views
BEGIN SELECT RAISE(ABORT,'generated memory views are immutable'); END;
CREATE TRIGGER memory_views_no_delete BEFORE DELETE ON memory_views
BEGIN SELECT RAISE(ABORT,'generated memory views are immutable'); END;
CREATE TRIGGER memory_view_sources_no_update BEFORE UPDATE ON memory_view_sources
BEGIN SELECT RAISE(ABORT,'generated memory view sources are immutable'); END;
CREATE TRIGGER memory_view_sources_no_delete BEFORE DELETE ON memory_view_sources
BEGIN SELECT RAISE(ABORT,'generated memory view sources are immutable'); END;
