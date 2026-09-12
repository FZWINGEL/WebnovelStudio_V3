-- Schema 39 gives stored documents an explicit authority role.  Existing
-- rows remain ordinary story documents; assistant drafts and control anchors
-- are isolated from ordinary story reads by the core reader boundary.
ALTER TABLE documents ADD COLUMN role TEXT NOT NULL DEFAULT 'ordinary'
    CHECK(role IN ('ordinary','assistantDraft','conversationAnchor'));

CREATE INDEX documents_role_position_idx ON documents(role, trashed, position, id);

CREATE TRIGGER documents_role_immutable
BEFORE UPDATE OF role ON documents
WHEN NEW.role IS NOT OLD.role
BEGIN SELECT RAISE(ABORT,'Document authority roles are immutable'); END;
