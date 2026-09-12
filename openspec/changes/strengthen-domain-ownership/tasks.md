## 1. Conversation query ownership

- [x] 1.1 Add the three conversation-owned run readers and internal output row type; verify rowid ordering, completed/delivered filtering, exact operation identity, and transaction visibility/rollback in focused Rust tests.
- [x] 1.2 Forward the readers through WorkshopHost and pass the active connection through candidate/provenance/impact helpers; verify no Workshop run-table SQL remains and existing stale/recovered/linked-adoption integrations pass.
- [x] 1.3 Add a lost-Workshop-receipt recovery regression using a synthetic trigger; verify exact original-run replay after state advancement and changed-payload collision refusal.

## 2. Frontend lifecycle ownership

- [x] 2.1 Introduce the document-workspace owner with private project/session, create/adoption tokens, tab/mode and export state; verify existing Workspace lifecycle and adoption regressions plus owner-level stale-completion/save-failure tests.
- [x] 2.2 Introduce the library owner with snapshot/forms and project-operation identities, using only explicit document-navigation capabilities; verify cancellation, retry identity and post-reconciliation lease behavior.
- [x] 2.3 Reduce workspaceModel to operation/presentation composition and Workspace to layout with a focused view/action surface; verify TypeScript and existing Workspace rendering/navigation/close tests without exposing raw project/session setters.
- [x] 2.4 Extend AST architecture tests for layout/owner dependency directions and mutation ownership; verify allowed and violating fixtures and the real import graph.

## 3. Store and generated-contract integrity

- [x] 3.1 Encapsulate Workshop result/lock writes in store actions with read-only projections; verify deduplication, notifications, locked-edit refusal, watermark preservation, and the existing flush/adoption regressions.
- [x] 3.2 Reject conflicting declarations within/across binding groups and duplicate output filenames while allowing identical repeats; verify focused positive/negative tests and unchanged current generation.
- [x] 3.3 Compare the exact generated TypeScript file inventory and contents, and safely reconcile only obsolete marked outputs; verify temporary-directory missing/changed/extra/foreign-file cases and no writes on declaration failure.

## 4. Architecture enforcement and documentation

- [x] 4.1 Add a Rust source-aware Workshop run-table ownership guard; verify ordinary/raw/macro string negative fixtures, comment controls and current production sources.
- [x] 4.2 Publish docs/ARCHITECTURE.md with actual layers, query/table owners, transaction flow, frontend owners and invariant enforcement levels; verify source links and consistency with implemented APIs.
- [x] 4.3 Link the current map from docs/README.md and the historical migration document, and correct stale affected crate/host charters; verify historical measurements remain explicitly historical and current claims match source.

## 5. Integrated qualification

- [x] 5.1 Independently review the combined implementation against the design/acceptance matrix and repair findings; verify no missing task behavior, compatibility regression or unrelated checkout change remains.
- [x] 5.2 Run the pinned complete local check (tooling, formatting, strict workspace Clippy, Rust tests, TypeScript, production build and frontend tests); verify successful exit and record actual final counts.
- [x] 5.3 Build a fresh native executable and run synthetic Workshop, chat/workspace and app-close flows sequentially; verify successful reports and record executable hash and qualification limits.
- [x] 5.4 Complete the implementation/evidence report and status ledger, reconcile every task to executed evidence, and validate the OpenSpec change; verify all acceptance rows are satisfied with no deferred implementation.
