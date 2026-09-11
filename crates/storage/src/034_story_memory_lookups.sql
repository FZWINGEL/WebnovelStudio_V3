-- Schema 34 raises the reader floor for the optional reviewed-memory lookup
-- capability. The durable lookup tables already retain the immutable request,
-- result, packet, and source identity JSON needed for validation, so no new
-- tables or columns are required. Legacy NULL/absent capability fields keep
-- their exact serialized bytes.
SELECT 1;
