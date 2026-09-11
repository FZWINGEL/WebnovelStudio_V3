-- Reader boundary for Astra/low maintenance and versioned app-server receipts.
-- Earlier packet bytes and terminal receipts are not rewritten.
ALTER TABLE provider_results ADD COLUMN app_server_delivery_json TEXT;
ALTER TABLE memory_results ADD COLUMN app_server_delivery_json TEXT;

-- The durable authorization record for one external turn. It is owned by the
-- existing discussion/memory job, not a second scheduler. Restore/copy cannot
-- turn an old claim into new generation authority.
CREATE TABLE codex_app_server_dispatches (
    job_kind TEXT NOT NULL CHECK(job_kind IN ('discussion','memory')),
    job_id TEXT NOT NULL,
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    dispatch_json TEXT NOT NULL,
    turn_id TEXT,
    PRIMARY KEY(job_kind,job_id)
) STRICT;

CREATE TRIGGER codex_app_server_dispatch_identity_immutable
BEFORE UPDATE OF job_kind,job_id,packet_id,dispatch_json ON codex_app_server_dispatches
BEGIN SELECT RAISE(ABORT,'App-server dispatch identity is immutable'); END;

CREATE TRIGGER codex_app_server_turn_identity_immutable
BEFORE UPDATE OF turn_id ON codex_app_server_dispatches
WHEN OLD.turn_id IS NOT NULL AND NEW.turn_id IS NOT OLD.turn_id
BEGIN SELECT RAISE(ABORT,'App-server turn identity is immutable'); END;

CREATE TRIGGER codex_app_server_dispatch_no_delete
BEFORE DELETE ON codex_app_server_dispatches
BEGIN SELECT RAISE(ABORT,'App-server dispatch records are immutable'); END;
