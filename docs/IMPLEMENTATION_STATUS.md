# V3 implementation status

**Status date:** 5 September 2026

**Current branch:** `codex/v3-persistence`
**Overall:** in progress; the full V3 goal is not complete.

WebnovelStudio V3 targets English novel authoring, UI, and export. Translated-webnovel, wuxia, xianxia, cultivation, and related register or terminology are optional English writing styles. The default native UI now has the persistent Library/Workspace surface. W1 Rust structural scope validation and shared JS/Rust fixtures are implemented, W2 session/core receipts and reconciliation work is implemented, and W3 registry/transfer work is active. The explicit W0 sample editor remains session-only; these boundaries do not establish native qualification or completion of the full product.

## Repository and CI evidence

- The private repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3). Its default branch is `main`, whose current tip is `d0eebfd780e435c068ef1017cac580786360d36b`.
- The implementation is pushed as `73db1bc03e9a99eef1e5cdd31d8dbcb9be0dac53` on `codex/v3-persistence`. Its [CI run 33973213684](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973213684) has passed Ubuntu contracts; Windows contracts and native qualification are still running at this record's update.
- The previous [CI run 33971040177](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33971040177), for `b8d4cb8`, passed both contract jobs, Windows Clippy/tests, and native build; native smoke reached the UI but timed out on a CSS-hidden diagnostic label. The committed fix waits for attachment and checks the actual runtime through IPC.
- The historical [W0 qualification record](W0_QUALIFICATION.md) remains the record for that spike. Current implementation status and remaining gates are maintained here.

## Current work

| Area | Status | Evidence or boundary | Remaining |
| --- | --- | --- | --- |
| W0 native editor | Partial baseline retained | Explicit sample editor trial in the real Tauri/WebView2 window, Rust snapshot validation over IPC, shared fixtures, session-only feedback/replacement | English native author trial, minimum-window behavior, external Word paste, screen-reader use, native backup/export dialog journey, and broader qualification |
| W1 structural scope | In progress | Rust structural scope validation is implemented with fixtures | Complete validator evidence, mutation/failure coverage, and JS/Rust qualification |
| W2 project/session/save | In progress | Core/frontend sessions, receipts/reconciliation, and default persistent UI wiring are implemented | Complete file-backed persistence/reconciliation evidence and the broader author-trial integration; no full W2/V3 completion claim |
| W3 library/transfer | Active work | Registry, transfer, schema-2 migration, and persistent Library/Workspace flows are underway/partly wired | Complete library/recovery/A-trial contract and evidence |
| GitHub/CI | Implementation pushed; Windows CI running | Private repo, `main` default, `73db1bc` run 33973213684 passed Ubuntu contracts | Record the Windows contracts/native results |

## Current local evidence

The root wrapper passed rustfmt, workspace Clippy with `-D warnings`, 59 Rust tests (one additional child test is ignored because its parent invokes it in a subprocess), the TypeScript/Vite build, and 73 frontend tests. Review regressions cover scene-break caret saving, ignoring an unavailable last document, preserving the current writer lease when destination reads fail, and view/metadata acknowledgment fences. After rebuilding, the real Tauri/WebView2 smoke path passed 14/14 checks on WebView2 `152.0.4191.62`, covering two-project save/switch/reload and copy isolation, process kill/restart, project/document rename, exact caret/last-document restore, and archive/unarchive. The ignored `.local/native-results/report.json` is machine-local evidence. A separate native Windows dialog check saved a 55,151-byte synthetic backup through the Backup action. Native export/recovery dialogs, normal-close behavior, and the broader A, N, and W3 qualification gates remain open.

## Full completion checklist

The following checklist preserves the approved work-package order and gates. A package is complete only when its implementation, failure coverage, and named evidence gate are recorded. W2 completion does not complete A, B, C, or the full V3 goal.

### W0 — native editor spike and contract lock

**Gate:** initial N-spike evidence; E1 begins here. **Status:** partial/in progress.

- [x] Keep a real Tauri/WebView2 development window with the restricted Tiptap schema and persistent mounted editor instance.
- [x] Record the snapshot, identity, scope, canonicalization, hash, short local barrier, and session-history boundary in [ADR 0001](ADR_0001_EDITOR_CONTRACT.md).
- [x] Keep shared JS/Rust golden fixtures and real IPC snapshot validation.
- [x] Exercise session-only feedback, selection quotation/focus, preview/reject, local strict replacement, and undo/redo in the development surface.
- [ ] Complete the English native author trial, minimum-window/DPI behavior, external Word paste, native backup/export dialog journey, and assistive-technology trial.
- [ ] Record the remote CI rerun after the local native wait/query fix and close the nominated Windows configuration evidence.
- [ ] Do not claim W0, N, an installed release, or real-manuscript readiness from the current spike alone.

### W1 — canonical documents and structural scope validation

**Gate:** P foundation. **Status:** in progress.

Rust structural scope validation and its fixtures are implemented; the package remains open until the complete validator evidence, mutation/failure coverage, and JS/Rust qualification are recorded.

- [ ] Implement the restricted document schema and canonicalization/hash contract as shared JS/Rust behavior.
- [ ] Implement block identity rules and Unicode endpoint conversion for the supported English editor surface.
- [x] Implement the independent Rust structural token iterator and scope validator.
- [ ] Prove unchanged outside-scope text, marks, links, block style/identity, and scene boundaries.
- [ ] Cover surrogate-pair, combining-mark, ZWJ, repeated-occurrence, empty-block, inline-only, cross-paragraph, malformed, and oversized fixtures as internal Unicode robustness cases.
- [ ] Keep Rust free of a general ProseMirror-step interpreter.
- [ ] Record P evidence before treating W1 as complete.

### W2 — real project persistence and document session

**Gate:** P. **Status:** in progress.

Core and frontend session work, receipts/reconciliation, and default persistent Library/Workspace wiring are implemented. Broader file-backed qualification and author-trial integration remain open; W2 does not complete the full V3 goal.

- [x] Implement core file-backed project storage with an owned connection/session boundary.
- [x] Implement migrations, working documents, immutable checkpoints, command receipts, writer leases, and typed Save/Reconcile operations.
- [x] Implement frontend `DocumentSession` generation watermarks, immutable in-flight payloads, serialized saves, lifecycle identity, and error buffers.
- [x] Prove delayed acknowledgments cannot replace newer editor text.
- [x] Prove operation/payload idempotency, changed-payload rejection, stale-version/lease rejection, definite-save error retention, and uncertain-outcome fencing/reconciliation.
- [x] Carry project/document/session identity through callbacks and reject late callbacks.
- [x] Prove process interruption after commit and before acknowledgment recovers the correct body.
- [ ] Read back WAL/FULL/foreign-key configuration in file-backed tests.
- [ ] Complete and qualify the persistence integration across the UI and author trial; W2 does not complete the full V3 goal.

### W3 — library, free-order work, recovery, and A trial

**Gate:** M/P plus the development-native author trial. **Status:** active work.

- [x] Implement New/Open/Rename/Duplicate/Archive/Locate and blank note/character/chapter creation.
- [x] Persist last item/caret state, switch only after flush, and enforce project locks and captured ownership.
- [ ] Restore into a new recovered project with a new identity and active operation namespace; keep the original untouched on failure.
- [ ] Keep copied receipts historical and unable to authorize new operations.
- [x] Add the explicit UTF-8 `Export draft` action from a flushed, frozen source.
- [ ] Complete the A trial across two offline projects, restart/resume, recovery copy, and draft export.
- [ ] Keep V2 import deferred to F1 and do not expose a placeholder Import V2 action.

### W4 — persistent conversation and deterministic jobs

**Gate:** M/P; no B trial yet. **Status:** planned.

Integrate the planned C0–C3 context foundation before or alongside this package: frozen snapshots and exact eligible sources, source epoch, deterministic exact retrieval with dirty-index fallback, mandatory-budget refusal, scoped author guidance, and actual-packet receipts/inspector. These context packages do not replace the base save, Apply, lifecycle, or authority contracts.

- [ ] Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a deterministic mock provider.
- [ ] Cover delayed output, malformed structured output, partial failure, cancellation, Stop ordering, and exact repeatable suggestions.
- [ ] Keep provider code from mutating manuscript bodies.
- [ ] Preserve author-room/prose-context separation and safe-brief rules for deliberately transferred privileged material.
- [ ] Recover discussion and job state on reload and project switching; retry creates a linked new run.

### W5 — review cards, prepared snapshots, and single author Apply

**Gate:** M/P; B trial remains blocked until W6. **Status:** planned.

- [ ] Implement source-bound proposals, editable prepared versions, exact before/after preview, one-at-a-time Apply, and Reject.
- [ ] Bind each proposal to its context snapshot, exact target/scope, source epoch, policy, and context receipt; F2 alone owns reviewed authority.
- [ ] Use the short local mutation barrier, preflighted editor transaction, durable decision/before/after/receipt transaction, and saved-generation handoff.
- [ ] Add selection toolbar, context-menu action, and keyboard/menu alternative.
- [ ] Prove undecided suggestions remain unchanged, repeated Apply cannot mutate twice, stale work is refused, and selected edits cannot change neighboring text, style, or boundaries.
- [ ] Keep Apply all/batch Apply out of this package; F5 owns that later contract.

### W6 — lost acknowledgment, shared lifecycle, history, and interruption hardening

**Gate:** P with native reruns; closes the B trial gate. **Status:** planned.

- [ ] Reconcile pending operation IDs and latest heads after lost acknowledgment.
- [ ] Share one lifecycle guard across Apply, reconciliation, editor disposal, switching, close, and application-controlled reload.
- [ ] Add in-session history boundaries, significant undo/redo checkpoints, restart comparison, and explicit restore.
- [ ] Cover forced renderer loss, process interruption, Stop orderings, restore A while B runs, and old-or-new transaction outcomes.
- [ ] Include context snapshots, source epoch, policy, delivered packet, and receipt in restart/fence/Stop coverage; no late context operation may trigger an implicit paid retry.
- [ ] Retain the live buffer on disk-full and permission errors; never hide external retries or paid restarts.
- [ ] Run the E4 stale-proposal friction check and keep conservative staleness unless measured evidence supports a bounded alternative.
- [ ] Run the B feedback trial only after the durable Apply and lifecycle evidence is complete.

### W7 — explicit exports and packaged native qualification

**Gate:** M/P/N complete. **Status:** planned.

- [ ] Add Markdown beside draft TXT, frozen source manifests, omission preview, and separate working-draft export.
- [ ] Finish keyboard navigation, accessible labels, focus restoration, resizing, native dialogs, and offline installation.
- [ ] Remove test-only command access and embedded automation from shipping builds.
- [ ] Qualify the packaged Windows WebView for keyboard/dead-key input, clipboard, focus, accessibility, high DPI, long chapters, recovery, Unicode/formatted projections, and export omissions.
- [ ] Do not infer publication, reviewed-story readiness, or continuity validity from export.

### W8 — one qualified live provider

**Gate:** L. **Status:** planned.

- [ ] Qualify one exact provider/model/configuration with explicit model and supported traits.
- [ ] Qualify one deterministic, fully recorded context packet first; treat the C6 bounded read loop as additional qualification after the one-packet route.
- [ ] Cover streamed completion, refusal/truncation, authentication failure, broken/partial streams, Stop, process cleanup, and recovered terminal history.
- [ ] Qualify credential entry/storage and inspect logs/backups for leakage.
- [ ] Document opaque upstream retries and the limits of local idempotency; do not claim exactly-once external billing.
- [ ] Keep unqualified adapters unavailable and never silently substitute a provider/model.

### F1 — V2 migration

**Gate:** named migration evidence. **Status:** planned.

- [ ] Implement staged read-only import and reconciliation for the tested V2 schemas only.
- [ ] Import context evidence and rebuild projections without changing source or authority semantics.
- [ ] Preserve active semantics versus inert legacy evidence and retain the original V2 application/data.
- [ ] Do not present Import V2 before representative snapshots and reconciliation pass.

### F2 — reviewed story boundary

**Gate:** architecture scenario 5. **Status:** planned.

- [ ] Implement typed accepted rules/records, staged ready bundles, basis manifests, explicit exceptions, dependency links, and conservative suffix fences.
- [ ] Enable reviewed-source continuation and ready export only with explicit validity; do not imply exhaustive continuity.
- [ ] Retain sole ownership of reviewed authority; context packets and generated digests cannot accept canon.

### F3 — context quality

**Gate:** measured task-specific context evidence. **Status:** planned.

- [ ] Add source packing, author-room/prose-context separation, safe briefs, exact previous prose, aliases/search, and freshness checks.
- [ ] Own C4 generated digests and C5 thin temporal/relationship/thread views, with C5 depending on F2; add richer quality only after the evidence-first C0–C3 foundation.
- [ ] Measure omissions and permissions before claiming a memory or prompt improvement.

### F4 — narrative evaluation

**Gate:** independent author-labelled evaluation. **Status:** planned.

- [ ] Run retrieval, continuity, prose, and author-acceptance cases using frozen model/settings.
- [ ] Limit conclusions to the evaluated English tasks, genres, lengths, and models.

### F5 — batch Apply

**Gate:** B complete and separate atomicity evidence. **Status:** planned.

- [ ] Implement same-base disjoint preparation and one atomic Apply/decision transaction.
- [ ] Validate the common context snapshot, policy, and source epoch as part of the atomic batch.
- [ ] Cover overlap, repeated operation IDs, stale/already-decided members, and lost acknowledgment.
- [ ] Preserve individual Apply as a single explicit author decision.

## Story Context extension completion ledger

The maintained [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) and [first-slice plan](V3_STORY_CONTEXT_FIRST_SLICE.md) are an adopted design extension pending implementation. Their C0–C6 packages are part of the full V3 goal and preserve the base save, Apply, lifecycle, and reviewed-authority ownership. Every C package is currently planned; no C0–C6 completion is claimed.

| Package | Planned owner and scope | Status | Required evidence before completion |
| --- | --- | --- | --- |
| C0 | Before/alongside W4; freeze contracts and adversarial eligibility fixtures | Planned | Source classes, timestamps, disclosure boundaries, and digest restrictions are fixture-defined and rejected when invalid |
| C1 | Before/alongside W4; immutable source snapshots, exact retrieval, source epoch, and dirty-index fallback | Planned | Exact handles, stale-index rebuild, source-epoch invalidation, and cross-project isolation |
| C2 | Before/alongside W4; deterministic multi-resolution packet compilation, mandatory-budget errors, and actual-packet receipts | Planned | Verbatim/digest/directory coverage, omission reasons, exact submitted packet/options, and no silent truncation |
| C3 | Before/alongside W4; scoped author guidance and the context inspector | Planned | Guidance scope/versioning, Used/Available/Not included/Needs refresh states, and author-room/prose-context separation |
| C4 | F3; source-bound generated digests and richer quality without automatic canon | Planned | Rebuild, late-result, source-change, deletion, restore, and citation-range evidence; no paid autosave analysis |
| C5 | F3 after F2; thin temporal, relationship, and thread views | Planned | Reviewed-authority dependency, source-bound view rebuild, and quality evidence for the supported English tasks |
| C6 | W8 additional qualification; bounded provider-side read loop | Planned | Stop/budget/duplicate-event/crash boundaries, visible unknown outcomes, and fresh invocation labeling |

The public promise is layered: stored evidence, permitted available sources, the packet actually delivered, and what a model understood are separate states; the last requires evaluation. Context work does not authorize automatic canon or replacement of source text with a large rolling summary.

## Completion rule

The full V3 goal is complete only after the applicable W0–W8, F1–F5, and C0–C6 gates have their implementation, failure coverage, and evidence recorded. Current W1/W2 progress and active W3 work are necessary groundwork; they do not close the A writing trial, B feedback trial, C release qualification, live-provider qualification, migration, reviewed-story, context-quality, narrative-evaluation, context-extension, or batch-Apply gates.
