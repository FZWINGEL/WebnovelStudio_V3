ALTER TABLE project
    ADD COLUMN context_source_epoch INTEGER NOT NULL DEFAULT 0
    CHECK(context_source_epoch>=0);

CREATE TABLE view_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    document_id TEXT NOT NULL REFERENCES documents(id),
    head_version INTEGER NOT NULL CHECK(head_version>=0),
    head_body_hash TEXT NOT NULL,
    anchor_block_id TEXT NOT NULL,
    anchor_utf16_offset INTEGER NOT NULL CHECK(anchor_utf16_offset>=0),
    focus_block_id TEXT NOT NULL,
    focus_utf16_offset INTEGER NOT NULL CHECK(focus_utf16_offset>=0)
) STRICT;
