-- Reviewed snapshots pin the resolved reader position alongside the immutable
-- revision. Working and historical snapshots leave this nullable for legacy
-- compatibility; restricted reviewed reads compare it with their descriptor.
ALTER TABLE snapshot_sources
    ADD COLUMN reader_position INTEGER
    CHECK(reader_position IS NULL OR reader_position>=0);
