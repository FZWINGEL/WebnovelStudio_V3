-- Passage-backed author-reviewed character knowledge.  Nullable pairs keep
-- legacy review-stage and ready-bundle rows byte-compatible; an empty set is
-- represented by NULL after an explicit clear at the request boundary.
ALTER TABLE review_stages ADD COLUMN knowledge_json TEXT;
ALTER TABLE review_stages ADD COLUMN knowledge_hash TEXT;
ALTER TABLE ready_bundles ADD COLUMN knowledge_json TEXT;
ALTER TABLE ready_bundles ADD COLUMN knowledge_hash TEXT;
