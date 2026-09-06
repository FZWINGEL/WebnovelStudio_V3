-- Bounded, request-scoped lookup invocations.  These rows are separate from
-- provider_results: the legacy table has one terminal receipt per run and its
-- exact bytes remain the compatibility boundary for non-lookup discussions.
ALTER TABLE discussion_drafts ADD COLUMN lookup_json TEXT;

CREATE TABLE discussion_lookup_invocations (
    run_id TEXT NOT NULL REFERENCES discussion_runs(id),
    ordinal INTEGER NOT NULL CHECK(ordinal>=0 AND ordinal<=2),
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    source_epoch INTEGER NOT NULL CHECK(source_epoch>=0),
    policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
    allowance_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('prepared','claimed','needs_context','completed','failed','stopped','unknown')),
    expected_sequence INTEGER NOT NULL CHECK(expected_sequence>=0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(run_id,ordinal),
    UNIQUE(packet_id)
) STRICT;

CREATE TABLE discussion_lookup_results (
    run_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    event_id TEXT NOT NULL,
    expected_sequence INTEGER NOT NULL CHECK(expected_sequence>=0),
    assistant_text TEXT NOT NULL,
    response_json TEXT,
    binding_json TEXT,
    outcome TEXT NOT NULL CHECK(outcome IN ('completed','stopped','timed_out','output_limit','failed')),
    confirmed_stdin_bytes INTEGER NOT NULL CHECK(confirmed_stdin_bytes>=0),
    usage_json TEXT,
    cleanup TEXT NOT NULL CHECK(cleanup IN ('settled','unresolved')),
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(run_id,ordinal),
    UNIQUE(run_id,event_id),
    UNIQUE(packet_id),
    FOREIGN KEY(run_id,ordinal) REFERENCES discussion_lookup_invocations(run_id,ordinal)
) STRICT;

CREATE TABLE discussion_lookup_reads (
    run_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    read_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    result_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
    truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(run_id,ordinal,read_id),
    UNIQUE(run_id,read_id),
    FOREIGN KEY(run_id,ordinal) REFERENCES discussion_lookup_invocations(run_id,ordinal)
) STRICT;

CREATE TRIGGER discussion_lookup_results_no_update
BEFORE UPDATE ON discussion_lookup_results
BEGIN
    SELECT RAISE(ABORT,'Discussion lookup results are immutable');
END;
CREATE TRIGGER discussion_lookup_results_no_delete
BEFORE DELETE ON discussion_lookup_results
BEGIN
    SELECT RAISE(ABORT,'Discussion lookup results are immutable');
END;
CREATE TRIGGER discussion_lookup_reads_no_update
BEFORE UPDATE ON discussion_lookup_reads
BEGIN
    SELECT RAISE(ABORT,'Discussion lookup reads are immutable');
END;
CREATE TRIGGER discussion_lookup_reads_no_delete
BEFORE DELETE ON discussion_lookup_reads
BEGIN
    SELECT RAISE(ABORT,'Discussion lookup reads are immutable');
END;
