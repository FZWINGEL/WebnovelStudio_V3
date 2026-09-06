-- OpenAI-compatible memory runs retain transport evidence separately from
-- Codex stdin delivery.  NULL preserves all historical Codex rows exactly.
ALTER TABLE memory_results ADD COLUMN delivery_json TEXT;
