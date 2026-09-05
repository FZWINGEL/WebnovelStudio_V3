# WebnovelStudio V3 — first-slice implementation plan

**5 September 2026 · proposed work plan, not completed implementation**  
**Authority:** [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md). Section references below point to that document. Its evidence limits and source references apply here.

## 1. The smallest useful proof

Build one installed Windows application in which an author can **create project A from a character note, create B from a blank chapter, write offline, resume both, discuss a chapter, request a precise passage edit, inspect/edit/apply/reject suggestions, undo, stop a reply, restart after an interrupted acknowledgment, recover a backup into a new project, and export chosen text**.

The manuscript and conversations use the real Rust/SQLite persistence path from the beginning. The deterministic mock substitutes only for external model execution. It must not substitute an in-memory database or pretend an Apply succeeded in React. The first slice does not need a world simulator, automatic fact extraction, semantic search, a polished template catalogue, or every V2 feature.

The minimum editor includes paragraphs, limited headings, bold/italic/links, hard breaks, and scene breaks. Notes and chapters share this body model. The Library includes create/open/rename/duplicate/archive/locate and independent resume state. One editing window, one writer per project, conservative stale suggestions, TXT/Markdown export, and recovered-copy restore are intentional constraints.

The first slice may preserve an **author-reviewed ready snapshot** and export it, with review coverage explicitly author-only. It must not expose “Continue from reviewed story” as continuity-checked until the basis/issue machinery is implemented. Ordinary working-draft assistance remains available. V2 import is a separate adoption gate because the supplied archive does not contain its actual schema or a representative database.

## 2. Keep five kinds of evidence separate

| Gate | What it proves | What it does not prove |
|---|---|---|
| M — mocked UX proof | Author flows and understandable scopes with deterministic responses | External protocol reliability, billing behavior, prose quality |
| P — persistence correctness | Real file-backed transactions, CAS, idempotency, recovery and project ownership | Native IME/accessibility or narrative correctness |
| N — native qualification | Actual installed Windows/WebView editor, clipboard, focus, IME and packaging | Reliability of an untested provider or another OS |
| L — live-provider acceptance | One explicitly qualified provider/model configuration honors its advertised contract | Exactly-once upstream execution, all provider versions, full-novel quality |
| Q — narrative-quality evaluation | Measured usefulness of context and suggestions on selected author tasks | General quality across genres, languages, lengths, or all models |

M/P/N define the safe technical proof slice. L is additionally required before describing its real assistant as supported. Q informs later narrative features; it is not replaced by infrastructure pass counts. Do not retire V2 for an actual manuscript merely because M passes.

## 3. Dependency-ordered work packages

Each package should be reviewable without reading the whole application. Write its contract/fixtures before wiring the next package. Tests below are proposed completion criteria; none has been run as part of this architecture review.

### W0 — Lock the contracts and qualify the risky editor surface

**Dependencies:** none. **Gate:** initial N evidence; experiments E1/E2/E3 begin here.

Create the smallest Tauri window with the intended restricted Tiptap schema and a persistent editor instance. Record exact dependencies and runtime versions. Write a short ADR fixing snapshot authority, block IDs, UTF-16 anchors, inline versus structural scope, apply barrier, and recovered-copy restore. Keep generated DTOs and shared golden fixtures in the repository.

**Done when:** Microsoft Pinyin composition, a formatted cross-paragraph selection, repeated Chinese/emoji text, focus transfer to a selection composer, scene breaks, clipboard paste, and a strict replacement/undo cycle work in the actual desktop window. Capture failures as reproducible fixtures. A browser-only prototype does not close this package. This is a qualification spike, not the production shell.

### W1 — Canonical documents and scope validation

**Dependencies:** W0's representation decision. **Gate:** P foundation.

Implement the restricted document schema, canonicalization/hash contract, block identity rules, Unicode endpoint conversion, structural token iterator, and independent Rust scope validator. Implement JS strict replacement preparation in a detached state. Keep document concerns independent of Tauri, model providers, and SQLite.

**Done when:** shared JS/Rust fixtures agree on body hashes, quote boundaries, valid/invalid anchors, split/merge/move/copy IDs, and proposed result snapshots. Mutation tests must reject changes to every kind of unselected content: text, marks, links, block style/identity, and scene boundaries. Include surrogate-pair and combining/ZWJ cases, repeated occurrences, empty blocks, inline-only grants, cross-paragraph replacement, and malformed/oversized input. No general PM-step interpreter is introduced in Rust.

### W2 — Real project persistence and the document session

**Dependencies:** W1. **Gate:** P.

Build the core project's owned connection thread, migrations, working documents, immutable checkpoints, command receipts, writer leases, and typed Save/Reconcile operations. Add the frontend `DocumentSession` with immutable in-flight payloads, generation watermarks, serialized saves, and error buffers. Use file-backed SQLite in integration tests.

**Done when:** delayed acknowledgments never replace newer editor text; same operation/payload is idempotent; changed payload with reused ID fails; stale versions/leases fail; definite save errors retain the live buffer; uncertain outcomes fence/reconcile before further writes. Kill the process after commit but before acknowledgment and recover the correct current body. Read back WAL/FULL/foreign-key configuration in the test binary. These tests precede a polished editor toolbar.

### W3 — Library, free-order work, and project recovery

**Dependencies:** W2. **Gates:** M/P.

Implement New/Open/Rename/Duplicate/Archive/Locate, blank note/character/chapter creation, last item/caret/thread/composer persistence, and detach-after-flush switching. Add the OS project lock and normal second-launch activation behavior. Implement consistent backup and validated recovered-copy restore now, not after a real manuscript has been entrusted to the app.

**Done when:** architecture scenario 1 passes with delayed saves; two physical copies with the same project identity are handled explicitly; a second process cannot write the same project; missing paths never create replacement data; duplicate creates an independent identity; interruption at each restore staging boundary leaves the original intact. A registry failure leaves a completed project discoverable through Open; retrying the same create/import operation does not create a second project. Sample creation is explicitly chosen and never a bootstrap side effect.

### W4 — Persistent conversation and deterministic jobs

**Dependencies:** W2/W3. **Gates:** M/P.

Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a mock provider with controllable barriers. The mock supports delayed first output, malformed structured response, partial failure, slow cancellation, completion/Stop order reversal, and exact repeatable edit suggestions. Create no manuscript mutation path in the provider layer.

**Done when:** whole-chapter discussion uses the saved whole chapter; passage discussion displays the historical quote and limited scope; no automatic request fires on save/chapter creation; missing models leave writing usable; Stop targets the exact run; terminal states never regress; duplicated/reordered events reconstruct one output; reload and project switching recover discussion and job state. Explicit retry creates a linked new run, not a disguised resume.

### W5 — Review cards, prepared snapshots, and author application

**Dependencies:** W1/W2/W4. **Gates:** M/P.

Implement source-bound proposal preparation, editable prepared versions, exact diff preview, Apply/Reject, and disjoint same-base Apply selected batch. Implement the short local mutation barrier, preflighted in-place editor transaction, durable decision/before/after/receipt transaction, and saved-generation handoff. Add selection toolbar, context-menu action, and keyboard/menu alternative.

**Done when:** architecture scenarios 2 and 3 pass through real IPC/database commands. Applying one of three edits leaves the others undecided and stale. Repeated Apply cannot mutate twice, including with a new operation ID. Batch failure is all-or-nothing. A selected sentence cannot change neighboring paragraph text, style, or boundaries; a whole-chapter rewrite is explicitly different. Composition is allowed to finish before Apply, and no provider wait occurs behind the barrier.

### W6 — Lost acknowledgment, history, and interruption hardening

**Dependencies:** W3/W4/W5. **Gate:** P, with native reruns.

Implement in-session history boundaries, significant undo/redo checkpoints, post-restart comparison/explicit restore, and full reconciliation of pending operation IDs and latest heads. Add process-level crash tests and Windows process-tree fixture support. Prioritize fixing invariants over adding UI surface.

**Done when:** scenario 4 recovers a committed Apply once and preserves a later saved v62 despite an old receipt pointing to v61. Ctrl+Z after restart is not presented as durable historical undo. Scenario 6 passes both Stop orderings and restore A while B runs. Inject failure after each transaction statement, kill renderer/app at controlled points, and test disk-full/permission errors. No hidden external retry or automatic paid restart occurs.

### W7 — Exports, author-ready snapshots, and packaged native acceptance

**Dependencies:** W3/W5/W6. **Gates:** M/P/N complete.

Add TXT/Markdown exports from explicit frozen sources, simple author-reviewed snapshots with truthful coverage, omission preview, and separate working-draft export. Finish keyboard navigation, accessible labels, focus restoration, resizing, native dialogs, and offline NSIS/WebView2 installation. Remove test-only command access and embedded automation services from shipping builds.

**Done when:** the complete two-project writing/feedback/restart/backup/export journey runs in the installed application; all P0 matrix cases pass; IME/clipboard/NVDA or Narrator and high-DPI checks are recorded on the nominated Windows machine; exported Unicode and formatting projections match chosen revisions; no pending suggestion leaks into export. Publication is not inferred from exporting. Model configuration is unnecessary for all manual paths.

### W8 — One qualified live provider

**Dependencies:** W4/W6 and early E3 results. **Gate:** L.

Default candidate: the documented noninteractive Claude Code adapter, with explicit model/traits, isolated configuration, disabled tools, Windows Job Object containment, frozen application input, and honest internal-retry limitations. Qualify one exact installed version/configuration. If safe text-only isolation cannot be demonstrated, use the direct HTTP Responses adapter for the first supported live path and leave that CLI version disabled. This is an evidence-based fallback, not an invitation to add generic shell execution.

**Done when:** explicit opt-in live trials verify selected/reported model handling, supported/unsupported traits, streamed completion, refusal/truncation, authentication error, partial/broken stream, Stop behavior, process cleanup where applicable, and recovered terminal history. Qualify native key entry/credential storage and inspect logs/backups for credential leakage. Record where the CLI's final upstream prompt or internal retries are opaque. Explain that local cancellation/idempotency do not prove a single external billable attempt.

The mock remains the reproducible failure harness. A second live adapter is added only through the same contract suite; it does not get its own writing engine or domain workflow.

## 4. Follow-on packages before specific claims or adoption

| Package | Dependency and scope | Claim it enables |
|---|---|---|
| F1 — V2 migration | Obtain actual V2 schema/migrations/export contracts and sanitized representative databases; implement staged read-only import and reconciliation report; preserve active semantics versus inert legacy evidence | Safe migration of those tested schemas—not arbitrary versions or lossless inference of missing anchors |
| F2 — Reviewed story boundary | Implement typed accepted rules/records, staged ready bundles, basis manifests, explicit exceptions, dependency links and conservative suffix fences; run architecture scenario 5 | Reviewed-source continuation and current ready export with explicit validity—not exhaustive continuity |
| F3 — Context quality | Add task-specific source packing, author-room/prose-context separation, safe briefs, exact previous prose, aliases/search and freshness checks; evaluate permissions and omissions | Measurable task-appropriate context, not guaranteed knowledge safety or better novels |
| F4 — Narrative evaluation | Author-labeled retrieval/continuity cases and independent prose/author-acceptance trials in English/Chinese where supported | Evidence for a specific memory/prompt improvement; possible justification for embeddings or less conservative rebasing |

Basic exact source freezing and safe-context separation required for first-slice proposals are **not** deferred to F3. F3 expands the narrative corpus and task coverage. Similarly, backups and safe restores are already in W3; migration is not an excuse to postpone recovery.

V2 retirement for a real manuscript requires W7, the applicable provider gate when used, and F1's actual-manuscript reconciliation plus a successful backup recovery/export exercise. F2 is needed before promising the stronger continuity feature. Keep the original V2 snapshot/application until the author accepts those results.

## 5. Experiments that can change the architecture

| Experiment | Smallest decisive setup | Default and consequence |
|---|---|---|
| E1 — Native editor suitability | Restricted editor in actual Tauri window; real Pinyin, selection composer, clipboard, screen reader, resize/focus; repeat minimal failures on a supported WebView | Keep Tauri/Tiptap. Trial Electron with the same editor/core only for an unresolved release blocker; direct PM only if wrapper behavior is the blocker. |
| E2 — Snapshot/apply cost | Real IPC and file-backed FULL commits with approximately 20k and 250k UTF-16-unit fixtures, many marks/blocks, concurrent streaming; measure serialization, input-to-paint, barrier, memory | Keep snapshots and barrier. Targets in §14 are proposals. Optimize renders/copies first; only sustained measured failure justifies an incremental protocol. |
| E3 — CLI containment and honesty | Exact executable/version; isolated temp working directory; attempt unexpected config/tool loading; child/grandchild fixture; Stop and internal-retry observation | Support only qualified modes. HTTP is preferable to pretending undocumented CLI controls exist. Billing/exactly-once guarantees remain out of scope. |
| E4 — Conservative conflict burden | Authors type while feedback runs, apply one of three edits, and encounter an earlier-chapter review fence; record confusion and manual refresh work | Keep conservative policy initially. Design bounded rebase/selective invalidation only for a demonstrated burden and prove it against the existing adversarial suite. |
| E5 — Memory value | Labeled exact evidence/knowledge-boundary tasks plus author review of paired outputs using frozen model/settings; compare plain recent prose, exact-state/search, and any proposed richer memory | Add a feature only for a measured failure it fixes. QA accuracy and attractive examples alone do not establish long-novel quality. |

## 6. Review and completion discipline

Every PR should identify the affected invariant, contract fixture, and failure test. Keep core transaction logic reviewable without Tauri or provider credentials. Keep a small fault-injection API behind test-only builds so lost acknowledgments and race orderings remain reproducible. Review scopes and normalization together whenever adding an editor node/extension.

No delivery date is estimated here. Calendar estimates would require actual team capacity, Rust/PM familiarity, target-machine access, and the V2 schema evidence. The critical path is **representation → persistence/reconciliation → scoped application → native failure qualification**, not the number of screens. Live provider breadth and narrative intelligence are separate work, not prerequisites for safe offline writing.

**First-slice success:** a real desktop writing session survives the specified failures with understandable author choices and no silent manuscript overwrite. **Not first-slice success:** an attractive mock, a passing browser suite, or a model that produces an impressive chapter once.
