-- Claude author receipts retain the model identity claimed by the terminal
-- stream separately from the requested immutable binding. NULL preserves
-- every historical Codex and HTTP receipt exactly.
ALTER TABLE provider_results ADD COLUMN reported_model TEXT;
