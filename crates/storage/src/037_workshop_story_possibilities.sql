-- Schema 37 raises the reader floor for optional typed story possibilities
-- in Workshop session/context JSON. Existing state and packet rows remain
-- byte-identical; readers must understand the new optional field before
-- reopening newer immutable records.
SELECT 1;
