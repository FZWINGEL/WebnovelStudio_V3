-- Durable author-room discussion threads, runs, messages, and composer drafts.
-- A run never owns a renderer lease.  The actor serializes all run changes;
-- output events are immutable and their sequence is the CAS boundary.
CREATE TABLE discussion_threads (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(project_id,operation_namespace,document_id)
) STRICT;

CREATE TABLE discussion_runs (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES discussion_threads(id),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    target_document_id TEXT NOT NULL REFERENCES documents(id),
    target_version INTEGER NOT NULL CHECK(target_version>=0),
    target_body_hash TEXT NOT NULL,
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    previous_run_id TEXT REFERENCES discussion_runs(id),
    status TEXT NOT NULL CHECK(status IN ('queued','running','stopping','completed','stopped','failed','interrupted')),
    dispatch_state TEXT NOT NULL DEFAULT 'pending' CHECK(dispatch_state IN ('pending','claimed','delivered')),
    sequence INTEGER NOT NULL DEFAULT 0 CHECK(sequence>=0),
    output_text TEXT NOT NULL DEFAULT '',
    stop_reason TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(project_id,operation_namespace,operation_id),
    UNIQUE(project_id,operation_namespace,id)
) STRICT;

CREATE TABLE discussion_messages (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES discussion_threads(id),
    run_id TEXT REFERENCES discussion_runs(id),
    role TEXT NOT NULL CHECK(role IN ('user','assistant')),
    content TEXT NOT NULL,
    scope_json TEXT,
    packet_id TEXT REFERENCES context_packets(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;

CREATE TABLE discussion_output_events (
    run_id TEXT NOT NULL REFERENCES discussion_runs(id),
    sequence INTEGER NOT NULL CHECK(sequence>0),
    event_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('chunk','terminal')),
    chunk TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(run_id,sequence),
    UNIQUE(run_id,event_id)
) STRICT;

CREATE TABLE discussion_drafts (
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    version INTEGER NOT NULL CHECK(version>=0),
    text TEXT NOT NULL,
    scope_json TEXT,
    pinned_document_ids_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(project_id,operation_namespace,document_id)
) STRICT;

CREATE TABLE discussion_draft_receipts (
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    expected_version INTEGER NOT NULL CHECK(expected_version>=0),
    payload_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(project_id,operation_namespace,operation_id)
) STRICT;

CREATE TRIGGER discussion_messages_no_update BEFORE UPDATE ON discussion_messages
BEGIN SELECT RAISE(ABORT,'Discussion messages are immutable'); END;
CREATE TRIGGER discussion_messages_no_delete BEFORE DELETE ON discussion_messages
BEGIN SELECT RAISE(ABORT,'Discussion messages are immutable'); END;
CREATE TRIGGER discussion_events_no_update BEFORE UPDATE ON discussion_output_events
BEGIN SELECT RAISE(ABORT,'Discussion output events are immutable'); END;
CREATE TRIGGER discussion_events_no_delete BEFORE DELETE ON discussion_output_events
BEGIN SELECT RAISE(ABORT,'Discussion output events are immutable'); END;
CREATE TRIGGER discussion_draft_receipts_no_update BEFORE UPDATE ON discussion_draft_receipts
BEGIN SELECT RAISE(ABORT,'Discussion draft receipts are immutable'); END;
CREATE TRIGGER discussion_draft_receipts_no_delete BEFORE DELETE ON discussion_draft_receipts
BEGIN SELECT RAISE(ABORT,'Discussion draft receipts are immutable'); END;
