-- Application conversation projections. Existing messages, immutable revisions,
-- and discussion runs remain the authorities for text and provider execution.
CREATE TABLE project_conversations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    anchor_document_id TEXT NOT NULL UNIQUE REFERENCES documents(id),
    composer_version INTEGER NOT NULL DEFAULT 0 CHECK(composer_version >= 0),
    composer_json TEXT NOT NULL,
    view_state_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(project_id, operation_namespace)
);
CREATE TABLE conversation_items (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES project_conversations(id),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    operation_id TEXT,
    kind TEXT NOT NULL,
    reference_id TEXT,
    payload_json TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(conversation_id, sequence),
    UNIQUE(conversation_id, operation_id)
);
CREATE INDEX conversation_items_reference ON conversation_items(conversation_id, kind, reference_id);
CREATE TRIGGER conversation_items_immutable_update BEFORE UPDATE ON conversation_items
BEGIN SELECT RAISE(ABORT,'Conversation history is immutable'); END;
CREATE TRIGGER conversation_items_immutable_delete BEFORE DELETE ON conversation_items
BEGIN SELECT RAISE(ABORT,'Conversation history is immutable'); END;
CREATE TABLE assistant_drafts (
    document_id TEXT PRIMARY KEY REFERENCES documents(id),
    conversation_id TEXT NOT NULL REFERENCES project_conversations(id),
    project_id TEXT NOT NULL,
    operation_namespace TEXT NOT NULL,
    origin_run_id TEXT NOT NULL REFERENCES discussion_runs(id),
    output_ordinal INTEGER NOT NULL CHECK(output_ordinal >= 0 AND output_ordinal < 3),
    packet_id TEXT NOT NULL REFERENCES context_packets(id),
    source_epoch INTEGER NOT NULL,
    policy_epoch INTEGER NOT NULL,
    target_json TEXT,
    initial_revision_id TEXT NOT NULL REFERENCES revisions(id),
    predecessor_document_id TEXT REFERENCES assistant_drafts(document_id),
    disposition TEXT NOT NULL DEFAULT 'pending' CHECK(disposition IN ('pending','rejected','adopted','superseded')),
    disposition_version INTEGER NOT NULL DEFAULT 0 CHECK(disposition_version >= 0),
    UNIQUE(origin_run_id, output_ordinal)
);
CREATE TRIGGER assistant_draft_provenance_immutable BEFORE UPDATE ON assistant_drafts
WHEN NEW.document_id != OLD.document_id OR NEW.conversation_id != OLD.conversation_id
  OR NEW.project_id != OLD.project_id OR NEW.operation_namespace != OLD.operation_namespace
  OR NEW.origin_run_id != OLD.origin_run_id OR NEW.output_ordinal != OLD.output_ordinal
  OR NEW.packet_id != OLD.packet_id OR NEW.source_epoch != OLD.source_epoch
  OR NEW.policy_epoch != OLD.policy_epoch OR NEW.target_json IS NOT OLD.target_json
  OR NEW.initial_revision_id != OLD.initial_revision_id
  OR NEW.predecessor_document_id IS NOT OLD.predecessor_document_id
BEGIN SELECT RAISE(ABORT,'Assistant draft provenance is immutable'); END;
CREATE TRIGGER assistant_draft_role BEFORE INSERT ON assistant_drafts
WHEN NOT EXISTS(SELECT 1 FROM documents WHERE id=NEW.document_id AND role='assistantDraft')
BEGIN SELECT RAISE(ABORT,'Assistant draft requires an isolated document'); END;
CREATE TRIGGER conversation_anchor_role BEFORE INSERT ON project_conversations
WHEN NOT EXISTS(SELECT 1 FROM documents WHERE id=NEW.anchor_document_id AND role='conversationAnchor')
BEGIN SELECT RAISE(ABORT,'Conversation requires a control anchor'); END;
CREATE TRIGGER conversation_identity_immutable BEFORE UPDATE ON project_conversations
WHEN NEW.id != OLD.id OR NEW.project_id != OLD.project_id
  OR NEW.operation_namespace != OLD.operation_namespace OR NEW.anchor_document_id != OLD.anchor_document_id
BEGIN SELECT RAISE(ABORT,'Conversation identity is immutable'); END;
-- Local draft/composer/adoption receipts and generation operations share the
-- author's operation namespace, but never share one operation identity.
CREATE TRIGGER chat_receipt_generation_collision BEFORE INSERT ON command_receipts
WHEN EXISTS(SELECT 1 FROM discussion_runs WHERE operation_namespace=NEW.operation_namespace AND operation_id=NEW.operation_id)
BEGIN SELECT RAISE(ABORT,'Operation identity already belongs to generation'); END;
CREATE TRIGGER generation_chat_receipt_collision BEFORE INSERT ON discussion_runs
WHEN EXISTS(SELECT 1 FROM command_receipts WHERE operation_namespace=NEW.operation_namespace AND operation_id=NEW.operation_id)
BEGIN SELECT RAISE(ABORT,'Operation identity already belongs to a local command'); END;
