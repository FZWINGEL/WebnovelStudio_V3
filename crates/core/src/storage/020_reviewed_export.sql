-- Reviewed exports retain the immutable ReadyBundle that authorized their
-- source. Existing working-draft rows remain NULL and keep their old bytes.
ALTER TABLE export_records ADD COLUMN review_bundle_id TEXT;
