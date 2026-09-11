-- Locally prepared requests. No provider call occurs when a row is inserted.
-- The job supervisor later references the exact prepared packet at dispatch.
CREATE TABLE context_packets (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    session_id TEXT NOT NULL,
    invocation_ordinal INTEGER NOT NULL CHECK(invocation_ordinal>=0),
    packet_json TEXT NOT NULL,
    packet_hash TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(operation_namespace,operation_id),
    UNIQUE(session_id,invocation_ordinal)
) STRICT;
CREATE TRIGGER immutable_context_packet_update BEFORE UPDATE ON context_packets
BEGIN SELECT RAISE(ABORT,'prepared context packets are immutable'); END;
CREATE TRIGGER immutable_context_packet_delete BEFORE DELETE ON context_packets
BEGIN SELECT RAISE(ABORT,'prepared context packets are immutable'); END;
