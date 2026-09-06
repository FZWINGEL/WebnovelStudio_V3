-- Explicit author-entered promise observations. Possession evidence remains
-- in its original nullable columns so legacy bytes and hashes are unchanged.
ALTER TABLE review_stages ADD COLUMN promises_json TEXT;
ALTER TABLE review_stages ADD COLUMN promises_hash TEXT;
ALTER TABLE ready_bundles ADD COLUMN promises_json TEXT;
ALTER TABLE ready_bundles ADD COLUMN promises_hash TEXT;
