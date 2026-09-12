## Context

See [proposal.md](proposal.md) for motivation. This design starts from
`arch-module` at `c91151b`, with the six local review fixes already applied and
verified against `codex/v3-persistence` at `97d0fe0`. The cleanly separated
production packages already obey the Cargo layer graph. The current source
still has these concentrated ownership gaps:

| Area | Observed coupling | Consequence |
| --- | --- | --- |
| Workshop results | Three direct `discussion_runs` queries in `workshop.rs` and `workshop/state.rs` | A conversation schema/query change requires editing its sibling |
| Candidate provenance | Completed-output queries execute during immediate save/adoption transactions | Moving the query outside the transaction could validate another snapshot |
| Workspace | One hook owns library, project/session, create intents, adoption, export, forms and display state | File extraction has not reduced lifecycle coupling |
| Workshop store | Shell assigns `results` and `locked` directly | Mutation ownership and notification rules are implicit |
| Bindings | Duplicate names are first-wins; drift only visits expected files | Conflicts or orphaned generated modules can be missed |
| Documentation | Crate charters still say skeleton; the target graph has historical ordinals | Readers cannot reliably distinguish current structure from aspirations |

The current check baseline is 943 passing Rust tests plus one intentional
ignore, 745 frontend tests, and 25 tooling tests. The baseline is an execution
checkpoint, not a completion criterion for new tests. CodeGraph currently
resolves the other worktree and is not used as evidence for this design.

## Goals / Non-Goals

**Goals:**

- Make the conversation package own the three run queries Workshop consumes.
- Keep every transaction, query ordering/filter, validation policy, error,
  operation identity and historical receipt compatible.
- Give library and document lifecycles separate owners with state and behavior
  together. Shell composition must not receive raw project/session setters.
- Retain the current editor until navigation's save and preparation barriers
  succeed, and fence asynchronous acknowledgments by project/session identity.
- Reject binding ambiguity and obsolete generated modules deterministically.
- Make these boundaries visible in current docs and regression checks.

**Non-Goals:**

- No new database schema, rewritten history, IPC command, provider policy,
  product mode, actor, or crate.
- No generic repository, dependency-injection container, global frontend store,
  event bus, or second editor engine.
- This increment does not promise to remove all raw SQLite access above L1 or
  replace every Apply implementation with a universal adoption service. Those
  targets affect many existing atomic workflows and need a separate justified
  design. The documentation will state the actual remaining boundary.

## Decisions

### 1. Conversation owns run selection; Workshop owns candidate interpretation

Add conversation-owned read operations for: ordered run IDs, a run ID by exact
project/operation namespace/operation ID, and ordered completed-and-delivered
output rows. Keep the existing full `read_run` reader for result display.

Workshop's host forwards these operations from core. For provenance and impact
validation, pass a stateless reader accepting the caller's `&Connection`, which
also accepts the existing transaction by dereference. The host must not open
another connection, create another actor command, cache the result, or prefetch
before `BEGIN IMMEDIATE`. An internal row type may live in existing lower-layer
run vocabulary; it is not an IPC type and needs no serialization derive.

The raw-output reader deliberately remains distinct from `read_run`: candidate
scanning tolerates unrelated malformed packets, while displaying a run performs
the full existing validation. Keep that distinction, rowid ordering, completed
and delivered predicates, and current/historical candidate filtering unchanged.
Transfer's validator does not use this query; do not add a transfer dependency
or callback merely to make signatures uniform.

Alternatives considered: a Workshop-to-conversation Cargo dependency violates
the established sibling boundary; moving queries into storage would move domain
policy away from its owner; pre-materializing outputs before the transaction
weakens consistency; a generic repository layer adds unnecessary abstraction.

### 2. Document workspace owns the mounted session and its transitions

Introduce a document-workspace owner within `shell/`. It owns the opened project,
active document and session, current tab/mode, pending logical document creation,
adoption token, export capture, and document/rename form state. Keep project
rename and backup here because they require the latest reconciled writer lease
and `projectWrite`/lifecycle guards.

Its commands include activation, replacement/return navigation, document/tab/mode
selection, create/rename, adoption preparation/acceptance/restoration, and export
preparation/write. Raw state setters and mutable identity refs remain private.
Read-only view data and explicit actions are exposed to composition.

Navigation keeps the existing ordering: flush chat and Workshop; settle a
pending document intent; run the editor's detach-after-preparation barrier;
activate only on success. Cancelled pickers and failed saves retain the mounted
editor. An unresolved create retains its exact ID and payload for reconciliation.
Adoption retains the flushed source for failure recovery, rereads Working heads
after a receipt, and rejects a stale project/session/adoption completion.

Shell owns visual overlays, search, focus restoration, common notice/error/busy
state, and attached feature handles. It supplies a small set of operation and
presentation callbacks, not a bag of state setters. Owner events reset display
state at the same transitions as today. App-close remains the existing separate
owner and consumes the current session through a read capability.

An arbitrary maximum file size is not an acceptance condition. Related
transaction-like UI protocols stay together even when a function is substantial.
The goal is explicit ownership, not a new large file with the same public setters.

### 3. Library owns catalog loading and project-operation identities

Introduce a library owner for snapshot/loading, new-project/import forms, and
create/recover/duplicate operation IDs. It receives the document owner's narrow
project-navigation capabilities and the shared operation envelope. It does not
construct `DocumentSession`, set project/active state, or duplicate save barriers.

Creating, opening, recovering, duplicating, and resuming an import use the same
atomic replacement capability. Returning to the library uses the corresponding
clear-after-success capability. Rename recovery receives a resolved reopen path
instead of importing the library's mutable snapshot. Startup loading and catalog
refresh remain local operations with no provider dispatch.

Reduce the workspace model's public return value to what layout consumes, using
named views/actions where useful. Remove the obsolete runtime imports from
`Workspace.tsx`; layout must delegate mutation to its model.

### 4. Workshop mutations go through store actions

Replace direct shell assignments with explicit lock and provisional-result
operations. Give consumers read-only accessors for mutable store fields; ensure
only store actions update them. Preserve the working-generation watermark,
pending save payload, polling behavior and the previous flush-race fix.

Do not notify React subscribers during render. A lock projection needed for
event admission must be synchronized without introducing a render-time state
update or an effect window that permits an otherwise blocked edit. Verify the
chosen lifecycle point with the existing adoption/uncertainty tests.

### 5. Generated modules have one unambiguous owner

Identical repeated declarations remain legal. Differing declarations under the
same TypeScript name must produce an actionable error identifying the conflicting
name and sources, both within a group and before cross-group ownership is chosen.
Duplicate output filenames must also fail. Never silently choose the first
incompatible declaration.

The drift check compares the exact generated TypeScript file set and contents,
so a removed group cannot leave a stale importable file. The generator can remove
obsolete files carrying its own generated marker within its exact output
directory; refuse unexpected unowned files rather than deleting them. Compute
and validate all output before writing, so declaration conflicts cause no partial
regeneration. Preserve the current nine modules and wire declarations unchanged
unless a verified generator defect requires a documented correction.

### 6. Guard the boundaries at the level they can enforce

- Retain Cargo-metadata layer checks and add a source-aware check that Workshop
  production code has no `discussion_runs` table literal. The check must detect
  raw, ordinary and macro-contained string literals, ignore comments, and have
  negative/control fixtures. This protects an accidental schema dependency; it
  is not a SQL authorization sandbox or proof against deliberate obfuscation.
- Extend frontend architectural checks to prevent layout from regaining runtime
  IPC/session mutation imports and library code from acquiring session ownership.
  Exercise violating and allowed examples through the same parser used on source.
- Use behavioral tests for ordering, stale completions, retry identity and store
  notifications. Avoid tests that only assert arbitrary line counts or file names.

### 7. Current contracts and migration evidence are separate

Create `docs/ARCHITECTURE.md` as the current entry point, with actual layer
ordinals, package and table/query ownership, actor/transaction flow, frontend
owner contracts, generated-IPC workflow, and a matrix distinguishing structural
checks, runtime guarantees, and remaining targets. Link it from `docs/README.md`
and the historical modular-decomposition document. Preserve the latter's history
and baseline measurements rather than rewriting them as current facts.

Update affected crate and host charters to describe actual responsibility.
Record final commands, counts, native build identity, and limitations in an
implementation report and link it from the implementation ledger. Do not
overstate local mock qualification as hosted, live-provider, or release proof.

## Risks / Trade-offs

- **Transaction drift** → pass the active connection into readers and prove they
  observe uncommitted rows on that transaction and rollback normally.
- **More demanding full-run validation** → keep raw completed output scanning
  and full run materialization separate; retain malformed-unrelated-output cases.
- **Stale React closures or late acknowledgments** → colocate identity refs with
  owned state, retain existing fences, add deferred-completion regression tests.
- **Save/recovery identity loss** → keep pending intents with their owner until
  success or a proven precommit refusal; test lost-ack and changed-form paths.
- **Render loops from store notifications** → distinguish lock projection from
  user mutation, test notification/adoption behavior without rendering writes.
- **Overbroad generator cleanup** → validate output names and restrict cleanup to
  marked generated files in the resolved output directory, with temporary tests.
- **False confidence from structural checks** → document their scope and pair
  them with behavioral integration tests and fresh native mock flows.

## Migration Plan

1. Preserve the initial local fixes and record this plan before editing runtime
   code. All work stays in the architecture worktree.
2. Implement the independent Rust, frontend, and binding/store slices with
   focused tests. Keep the source graph buildable at integration boundaries.
3. Review the combined diff for changed error text, ordering, transaction scope,
   public surface, generated bytes, and accidental sibling dependencies.
4. Run formatting, strict Clippy, all Rust tests, frontend tests, TypeScript,
   production build, and tooling checks through the pinned wrapper.
5. Build a fresh native executable and run synthetic Workshop, chat/workspace,
   and app-close flows sequentially. Repair any regression and rerun the affected
   checks. Retain logs and exact binary identity under ignored `.local/` paths.
6. Complete the requirement/evidence ledger and current documentation. Do not
   mark tasks complete while an acceptance item is deferred or merely planned.

There is no data migration. Rollback, if needed, reverts only this increment's
source changes while preserving the six prior fixes and all author data. Commit
or publication is a separate user action.

## Acceptance Matrix

| Boundary | Required evidence |
| --- | --- |
| Conversation readers | Ordering, exact owner lookup, completed/delivered filtering, transaction visibility and rollback tests |
| Workshop consumption | No direct run-table reads, unchanged candidate/staleness/impact and replay integration tests |
| Workspace ownership | Private session/project mutation, library capability seam, layout import guard |
| Lifecycle compatibility | Save failure/cancel retains editor; create retries keep IDs; stale adoption/read cannot replace current editor |
| Store | Locked edits refused, provisional result deduplicates/notifies, polling and flush race regressions pass |
| Bindings | Same-name conflict rejection, identical duplicate acceptance, exact file-set drift, safe obsolete-output handling |
| Whole tree | Pinned full check succeeds and generated IPC files remain current |
| Native runtime | Fresh mock Workshop, chat/workspace, and app-close flows pass with retained build identity |
| Documentation | Current ownership/invariant map, historical target clearly marked, tasks reconciled to executed evidence |
