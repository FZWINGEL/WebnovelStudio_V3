-- Explicit passage-backed reviewed evidence.  Null/null is the legacy empty
-- set; nonempty arrays are canonical and hash-bound to each immutable stage
-- and bundle.
ALTER TABLE review_stages ADD COLUMN records_json TEXT;
ALTER TABLE review_stages ADD COLUMN records_hash TEXT;
ALTER TABLE ready_bundles ADD COLUMN records_json TEXT;
ALTER TABLE ready_bundles ADD COLUMN records_hash TEXT;
