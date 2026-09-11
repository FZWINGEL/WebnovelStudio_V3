-- Continuation proposals use the same immutable proposal/decision/receipt
-- chain as passage review.  The discriminator is durable so old candidate
-- bytes remain byte-for-byte legacy passage JSON and are never guessed from
-- their shape.
ALTER TABLE proposals
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'passage'
    CHECK (kind IN ('passage','continuation'));

ALTER TABLE proposal_versions
    ADD COLUMN payload_json TEXT;

-- A continuation draft is based on either the author's Working or Reviewed
-- document.  Rebuild the table because SQLite cannot extend the existing
-- intent CHECK constraint in place.  The copy deliberately carries every
-- pre-19 column, including updated_at, without normalizing historical data.
ALTER TABLE discussion_drafts RENAME TO discussion_drafts_before_continuation;

CREATE TABLE discussion_drafts (
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    version INTEGER NOT NULL CHECK(version>=0),
    text TEXT NOT NULL,
    scope_json TEXT,
    pinned_document_ids_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    previous_run_id TEXT REFERENCES discussion_runs(id),
    safe_brief_json TEXT,
    intent TEXT NOT NULL DEFAULT 'discuss'
        CHECK(intent IN ('discuss','proposeEdits','continue')),
    basis TEXT
        CHECK(basis IS NULL OR basis IN ('working','reviewed')),
    PRIMARY KEY(project_id,operation_namespace,document_id),
    CHECK((intent='continue' AND basis IS NOT NULL)
        OR (intent<>'continue' AND basis IS NULL))
) STRICT;

INSERT INTO discussion_drafts(
    project_id,operation_namespace,document_id,version,text,scope_json,
    pinned_document_ids_json,updated_at,previous_run_id,safe_brief_json,intent,basis
)
SELECT
    project_id,operation_namespace,document_id,version,text,scope_json,
    pinned_document_ids_json,updated_at,previous_run_id,safe_brief_json,intent,NULL
FROM discussion_drafts_before_continuation;

DROP TABLE discussion_drafts_before_continuation;
