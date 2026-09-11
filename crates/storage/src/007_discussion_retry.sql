-- Preserve the author's explicit retry choice with the unsent composer.
-- The link grants no dispatch permission; start revalidates its exact request,
-- source policy, guidance, project, and operation namespace.
ALTER TABLE discussion_drafts ADD COLUMN previous_run_id TEXT REFERENCES discussion_runs(id);
