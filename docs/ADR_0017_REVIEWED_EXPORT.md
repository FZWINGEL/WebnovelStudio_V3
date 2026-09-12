# ADR 0017: Export an author-reviewed chapter snapshot

Status: implemented and CI-qualified as a development slice. The local
wrapper passes, and CI 34010306332 passes both contract jobs and all 38 native checks.
This extends the author-only review records in [ADR 0012](ADR_0012_AUTHOR_REVIEW.md)
and the existing single-document export path. It does not add publication,
canon, or continuity authority.

## Author outcome

After explicitly marking a chapter reviewed, the author may choose **Export
author-reviewed chapter** and save one exact Markdown or plain-text snapshot.
The UI must label this as an **author-reviewed snapshot**. It must not call the
file published, canonical, accepted, or finished narrative. The export is the
author's reviewed prose plus the existing format-loss disclosure.

Working-draft export remains available and keeps its existing request bytes,
preview bytes, hashes, receipts, and records unchanged. Reviewed export does
not invoke a model, mutate the manuscript, or create a checkpoint.

## Basis and prepare contract

Add a dedicated `prepare_reviewed_draft_export` command. The command accepts
the current project access, expected chapter head, and `DraftFormat`; Rust
resolves the active `ready_heads` entry and its immutable `ReadyBundle`. A
client-supplied bundle ID is evidence to return in the preview, never the
authority for selecting a bundle.

Preparation succeeds only when all of these match:

- the document is a current, non-trashed chapter in the caller's project and
  operation namespace;
- the selected `ReadyBundle` is the current head for that chapter;
- the bundle target head, target revision, and exact saved body hash match the
  expected head and retained revision;
- the bundle has `authorOnly` coverage and its ordered earlier prefix is still
  valid under the current disclosure policy and selected reviewed heads;
- the chapter review status is `Ready`, including target, earlier-basis, and
  policy checks.

No fallback to a working export is allowed. Missing review, changed prose,
changed earlier basis, policy change, foreign identity, and recovered-copy
authority loss are explicit refusals.

The returned `DraftExportPreview` reuses the existing projection, UTF-8 byte
count, SHA-256, format version, source head, and revision fields. It adds an
optional immutable `reviewBundleId`; the field is absent for working previews
so legacy serialized previews and hashes remain byte-compatible.

## Durable record and install

Schema 20 adds nullable `export_records.review_bundle_id`. Existing rows retain
`review_bundle_id = NULL` and `working_draft = 1`. A reviewed record stores the
resolved bundle ID and `working_draft = 0`. Rust validation must require the
matching combination and independently verify the immutable bundle, target
revision, source hash, and exact projected bytes.

Reuse `export_prepared_draft`, the preview integrity checks, native destination
dialog, no-overwrite file installation, and immutable export receipt. The
native save dialog only chooses a destination. After it returns, the owned
project actor performs the final reviewed-bundle/head, earlier-basis,
namespace, policy, revision, and projected-byte/hash validation immediately
before staging and linking the file. That successful actor-side validation is
the freshness acceptance point. If the author changes the chapter, changes an
earlier reviewed basis, or policy changes while the dialog is open, the
operation fails before creating a destination.

The source is not locked forever after acceptance. Later edits do not change
the already accepted bytes or their historical receipt. Filesystem installation
and SQLite receipt recording remain separate failure boundaries; this is not an
atomic filesystem-plus-database transaction. A destination race or validation
failure leaves no destination, and the existing private staging cleanup applies.
If the no-overwrite file link succeeds but receipt recording fails or is
uncertain, preserve the installed file, report the explicit unavailable-record
boundary, and require a fresh explicit preview rather than claiming rollback.
The implementation and native gate must exercise this boundary instead of
assuming that filesystem and SQLite can be rolled back together.

Historical reviewed export records remain independently verifiable after a
newer bundle becomes selected or the old bundle is no longer current. Reading
history must not reactivate that bundle or authorize a new export. A recovered
or duplicated project cannot inherit review authority: its active reviewed
heads are cleared, and old bundles remain historical evidence only.

## UI and ownership

Keep the existing working export action. Add one explicit reviewed action (or a
clearly separated basis choice) only for chapters, with the author-reviewed
label and the current refusal explanation. Reuse `ExportDialog` for format,
preview, destination choice, focus restoration, and save feedback.

The bounded implementation owns:

- `crates/core/src/projects/reviewed_story.rs` for current-bundle resolution
  and exact basis validation;
- `crates/core/src/transfer.rs` and `crates/core/src/projects/exports.rs` for
  preview/install and durable reviewed metadata;
- schema 20 migration and backup/reader-floor validation;
- `apps/desktop/src-tauri/src/export_commands.rs`, `apps/desktop/src/ipc/exports.ts`,
  `Workspace.tsx`, and `ExportDialog.tsx` for the explicit action.

Typed facts, accepted summaries, multi-chapter/collection export, publication
state, and narrative-quality claims remain outside this package.

## Boundary tests and qualification

The focused contract suite should cover:

1. A reviewed preview uses the exact bundle revision and creates no checkpoint,
   including a reviewed first chapter with an empty earlier prefix.
2. No review, changed target, changed earlier basis, and policy
   revocation refuse without working-draft fallback.
3. A preview made current becomes stale during destination selection and writes
   no file.
4. Markdown/TXT bytes and hashes equal the existing projection for the exact
   reviewed revision.
5. Legacy working previews, records, receipts, and hashes remain readable and
   unchanged after schema 20.
6. A historical record remains verifiable after the selected head advances,
   while a new reviewed export requires the new current bundle.
7. Foreign/tampered bundle IDs and recovered or duplicated namespaces cannot
   authorize reviewed export.
8. UI/native flow shows the author-reviewed label, saves exact chosen bytes,
   preserves the working action, and reports stale refusal without a file.

Feature completion is the deterministic core, migration, IPC, UI, and focused
test contract. A separate local WebView2/native gate must run the visible
review, export, exact readback, reopen/history, stale-refusal, and recovery
journey. Live-provider qualification is irrelevant to this export feature and
must not be inferred from it; broader release/package qualification remains a
separate gate.

The rebuilt local native diagnostic now covers this reviewed-export journey in
37 of 38 checks with zero errors; only the OS clipboard check is omitted.
Evidence is
`.local/native-other-results/report.json` from `2026-09-06T03:53:42.453Z`,
using WebView2 `152.0.4191.62`; the executable is SHA-256
`456a0cf1c327c330c0f56e4e472630e5dfeaf2976cb969cc17f052696208492d`,
29,184,000 bytes, built at `2026-09-06T03:51:09Z`. The flow covers first-chapter
reviewed Markdown/TXT export, exact readback, reopened history, copied-authority
refusal, recording-failure preservation, and stale refusal after the native Save
dialog. Root review found the Markdown readback and stale-refusal screens
readable. It made no new LLM calls. The bundle measured 690.55 KB JavaScript
and 33.29 KB CSS with the existing Vite chunk warning.

The local wrapper also passes 435 active Rust tests (411 core and 24 desktop,
with one existing ignored), 256 frontend tests in 21 files, workspace rustfmt,
Clippy with `-D warnings`, TypeScript, and Vite. This is local development
evidence. [CI 34010306332](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34010306332) passes both contract jobs and all 38 strict native checks, including clipboard, with zero errors. The downloaded report is `2026-09-06T04:10:43.182Z` on WebView2 `151.0.4129.101` at checkpoint `a19e7bf7d30746cc02cd2769e6295779a4bcbb4d`. Full provider, author-trial, and release qualification remain separate.
