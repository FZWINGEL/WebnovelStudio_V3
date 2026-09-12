-- Original source pins and generated-view pins have separate identities.
-- Old snapshot/packet JSON remains byte-for-byte unchanged.
CREATE TABLE snapshot_navigation_views (
    snapshot_id TEXT NOT NULL REFERENCES story_snapshots(id),
    view_id TEXT NOT NULL REFERENCES memory_views(id),
    content_hash TEXT NOT NULL,
    PRIMARY KEY(snapshot_id,view_id)
) STRICT;

CREATE TRIGGER snapshot_navigation_views_no_update BEFORE UPDATE ON snapshot_navigation_views
BEGIN SELECT RAISE(ABORT,'frozen navigation view pins are immutable'); END;
CREATE TRIGGER snapshot_navigation_views_no_delete BEFORE DELETE ON snapshot_navigation_views
BEGIN SELECT RAISE(ABORT,'frozen navigation view pins are immutable'); END;
