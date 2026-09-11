-- Immutable author-reviewed narrative summaries are nullable so historical
-- stages and bundles retain their exact legacy representation.
ALTER TABLE review_stages ADD COLUMN summary_json TEXT;
ALTER TABLE review_stages ADD COLUMN summary_hash TEXT;
ALTER TABLE ready_bundles ADD COLUMN summary_json TEXT;
ALTER TABLE ready_bundles ADD COLUMN summary_hash TEXT;
