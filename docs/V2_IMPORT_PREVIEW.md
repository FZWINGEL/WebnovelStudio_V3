# V2 import preview and staged installation

`webnovel_core::v2_import::list_v2_projects(path)` lists the bounded set of
projects in one explicitly chosen V2 database. The author then selects one
project and calls
`webnovel_core::v2_import::preview_v2_import(path, source_project_id)` for its
reviewable snapshot. The source must be a stable schema-8 database copy. The
reader is Windows-only for this slice: non-Windows calls return
`UnsupportedPlatform` before opening the source.

The preview retains the source schema and migration ledger, copied-byte
fingerprint, selected project identity, chapter ordering and retirement state,
working prose state, draft pointers, and inert legacy records with counts. It
validates the complete expected table and column contract, migration names,
database integrity, foreign keys, project ownership, and generic record
references before returning the result. Source size is capped at 512 MiB;
retained legacy JSON at 4 MiB and 20,000 rows. These limits cause refusal
rather than truncation.

On Windows the reader holds a read-sharing source handle that excludes
concurrent writers and deletion while copying. It copies bounded chunks into
an owned temporary directory, hashes the copied bytes, and reads only that
owned database inside one SQLite read transaction. Every exit path cleans the
copied database and known sidecars. The original database is never opened
through SQLite on this path. An active V2 writer must be closed or a stable
copy supplied. Persistent WAL mode is refused from the file header, even when
no sidecar remains. Portable non-Windows import requires separate
qualification.

`working_prose = NULL` and `working_prose = ''` are different states. Present
working prose, including an intentionally empty string, remains exact. A
chapter without working prose is marked as requiring an author choice; the
author must explicitly import empty text or select a draft belonging to that
same chapter. Draft approval flags are shown as history and do not silently
choose a V3 manuscript or establish reviewed authority. Retired chapters
remain visible in the preview.

The F1 installation boundary is
`Library::import_v2(V2ImportRequest)`. It rechecks the source fingerprint and
the complete request before staging a fresh project with new project and
operation namespace identities. It creates editable chapter documents from
the selected body decisions, copies bounded narrative notes into editable
documents, and stores the source manifest, source-to-V3 ID map, exact body
decisions, and all permitted legacy rows as inert historical evidence in the
project database. V2 approval or generation state never becomes V3 canon,
review authority, or executable work.

The library operation records the source fingerprint and a bounded versioned
request record containing the source project, request hash, and exact missing-
body choices before file installation. Repeating an operation ID with the same
request reconciles the original staging or destination and never creates a
second project. Completed replay and a pending lost-acknowledgment resume read
the target marker, creation record, and import manifest through one read-only
SQLite snapshot; they do not reopen the target writer or reread the original
V2 source. A valid sealed staging folder is checked before it is moved. This
permits recovery while the imported project has a running discussion, even if
the source file was later deleted. Changed requests, legacy incomplete rows
without retained choices, and tampered target evidence are refused with an
explicit recovery error.

Backups rotate only the current V3 project identity and operation namespace.
The original source project ID, source hashes, operation ID, and legacy rows
remain historical evidence; recovered-copy receipts cannot authorize a new
import. Backup validation checks the manifest, ID map, body decisions,
immutable imported revisions, and inert record ownership before installation.

The native boundary exposes `v2_import_list_projects`, `v2_import_preview`,
`v2_import`, and `library_resume_import`. The Library dialog keeps source
project selection and missing-body choices explicit, forwards the reviewed
source hash, and locks the reviewed request after an uncertain response so
**Check import** reuses the same operation ID and payload. Close remains
available after the request stops working; the Library's pending-operation
row then offers the same durable **Check import** action after restart.

Focused evidence for this slice is 15/15 `v2_import` tests, 24/24 transfer
tests, and 4/4 `V2ImportDialog` tests. Core importer Clippy checks with
`-D warnings` and desktop TypeScript checking pass. No real author V2 database
has been opened, and Windows release qualification remains a separate gate.

The native synthetic journey passed at `2026-09-05T22:52:29.674Z`: real Tauri
Library and source chooser, explicit same-chapter saved-draft choice, readable
imported note, independent V3 identity/namespace, unchanged source bytes, and
reopened chapter prose. Its executable SHA-256 was
`9c13a273210ab0fc890645f176cf8b8c6c8bd4b3cd8808f48bc47479862c6d1f`.
The ignored report and review/open/reopen screenshots are under
`.local/import-native-qualification/run-2026-09-05T22-52-27-636Z-30868/`.
Chooser automation and accessible-name mismatches in earlier harness attempts
were corrected; those failed attempts are retained separately. This establishes
one synthetic schema-8 native import journey, not author-data, installer,
non-Windows, or exhaustive pending-import recovery acceptance.
