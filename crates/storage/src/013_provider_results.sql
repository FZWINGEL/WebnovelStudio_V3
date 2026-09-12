-- Immutable provider terminal receipts for bounded live runs.
-- Raw provider diagnostics are never stored here; the core receives only the
-- sanitized terminal report and the exact binding captured by its packet.
CREATE TABLE provider_results (
    run_id TEXT PRIMARY KEY REFERENCES discussion_runs(id),
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    terminal_event_id TEXT NOT NULL,
    expected_sequence INTEGER NOT NULL CHECK(expected_sequence>=0),
    binding_json TEXT NOT NULL,
    assistant_text TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK(outcome IN ('completed','stopped','timed_out','output_limit','failed')),
    confirmed_stdin_bytes INTEGER NOT NULL CHECK(confirmed_stdin_bytes>=0),
    usage_json TEXT,
    cleanup TEXT NOT NULL CHECK(cleanup IN ('settled','unresolved')),
    error TEXT,
    effective_identity TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;

CREATE TRIGGER provider_results_no_update BEFORE UPDATE ON provider_results
BEGIN SELECT RAISE(ABORT,'Provider results are immutable'); END;
CREATE TRIGGER provider_results_no_delete BEFORE DELETE ON provider_results
BEGIN SELECT RAISE(ABORT,'Provider results are immutable'); END;
