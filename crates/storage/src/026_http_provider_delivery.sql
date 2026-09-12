-- Transport-specific delivery evidence for OpenAI-compatible HTTP runs.
-- Historical Codex rows retain their exact stdin contract and leave this
-- additive field NULL.
ALTER TABLE provider_results ADD COLUMN delivery_json TEXT;
