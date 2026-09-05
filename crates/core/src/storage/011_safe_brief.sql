-- Request-scoped author-approved writing briefs are durable draft state only.
-- Runs and immutable packets retain the exact value in their JSON requests.
ALTER TABLE discussion_drafts ADD COLUMN safe_brief_json TEXT;
