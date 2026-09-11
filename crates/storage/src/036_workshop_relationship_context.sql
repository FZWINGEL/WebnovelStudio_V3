-- Schema 36 raises the reader floor for optional typed relationship identity
-- in Workshop session/context JSON. Existing state, preview, and packet rows
-- remain byte-identical; the reader must understand the new optional fields
-- before reopening those immutable records.
SELECT 1;
