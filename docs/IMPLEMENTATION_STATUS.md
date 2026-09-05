# V3 implementation status

**Status date:** 5 September 2026

**Current branch:** `codex/v3-persistence`
**Overall:** in progress; the full V3 goal is not complete.

WebnovelStudio V3 targets English novel authoring, UI, and export. Translated-webnovel, wuxia, xianxia, cultivation, and related register or terminology are optional English writing styles. W1 and W2 are being implemented now, but the runtime UI remains the session-only W0 surface. W1/W2 work must not be described as integrated UI/persistence or as completion of the full product.

## Repository and CI evidence

- The private repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3). Its default branch is `main`, whose current tip is `d0eebfd780e435c068ef1017cac580786360d36b`.
- The completed CI run [33969395869](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33969395869) passed Ubuntu and Windows core/frontend contract jobs, Windows workspace Clippy/tests, and the native Tauri build.
- The same CI run did not close native qualification: native smoke failed before the UI because CDP startup exceeded 20 seconds and the connection was refused.
- The historical [W0 qualification record](W0_QUALIFICATION.md) remains the record for that spike. Current implementation status and remaining gates are maintained here.

## Current work

| Area | Status | Evidence or boundary | Remaining |
| --- | --- | --- | --- |
| W0 native editor | Partial baseline retained | Real Tauri/WebView2 editor, Rust snapshot validation over IPC, shared fixtures, session-only feedback/replacement | English native author trial, minimum-window behavior, external Word paste, screen-reader use, and broader native qualification |
| W1 structural scope | In progress | Rust structural scope validation is being implemented | Complete validator, shared fixtures, mutation/failure coverage, and cross-language qualification |
| W2 project/session/save | In progress | Core file-backed projects, sessions, and saves are being implemented | Complete persistence/reconciliation contract and integration with the UI; no integrated UI/persistence claim yet |
| GitHub/CI | Repository and baseline CI available | Private repo, `main` default, run 33969395869 | Repair native smoke startup/connection and rerun authoritative CI |

## Full completion checklist

The following checklist preserves the approved work-package order and gates. A package is complete only when its implementation, failure coverage, and named evidence gate are recorded. W2 completion does not complete A, B, C, or the full V3 goal.

### W0 — native editor spike and contract lock

**Gate:** initial N-spike evidence; E1 begins here. **Status:** partial/in progress.

- [x] Keep a real Tauri/WebView2 development window with the restricted Tiptap schema and persistent mounted editor instance.
- [x] Record the snapshot, identity, scope, canonicalization, hash, short local barrier, and session-history boundary in [ADR 0001](ADR_0001_EDITOR_CONTRACT.md).
- [x] Keep shared JS/Rust golden fixtures and real IPC snapshot validation.
- [x] Exercise session-only feedback, selection quotation/focus, preview/reject, local strict replacement, and undo/redo in the development surface.
- [ ] Complete the English native author trial, minimum-window/DPI behavior, external Word paste, and assistive-technology trial.
- [ ] Repair native smoke startup/connection and rerun the native evidence on the nominated Windows configuration.
- [ ] Do not claim W0, N, an installed release, or real-manuscript readiness from the current spike alone.

### W1 — canonical documents and structural scope validation

**Gate:** P foundation. **Status:** in progress.

- [ ] Implement the restricted document schema and canonicalization/hash contract as shared JS/Rust behavior.
- [ ] Implement block identity rules and Unicode endpoint conversion for the supported English editor surface.
- [ ] Implement the independent Rust structural token iterator and scope validator.
- [ ] Prove unchanged outside-scope text, marks, links, block style/identity, and scene boundaries.
- [ ] Cover surrogate-pair, combining-mark, ZWJ, repeated-occurrence, empty-block, inline-only, cross-paragraph, malformed, and oversized fixtures as internal Unicode robustness cases.
- [ ] Keep Rust free of a general ProseMirror-step interpreter.
- [ ] Record P evidence before treating W1 as complete.

### W2 — real project persistence and document session

**Gate:** P. **Status:** in progress.

- [ ] Implement core file-backed project storage with an owned connection/session boundary.
- [ ] Implement migrations, working documents, immutable checkpoints, command receipts, writer leases, and typed Save/Reconcile operations.
- [ ] Implement frontend `DocumentSession` generation watermarks, immutable in-flight payloads, serialized saves, lifecycle identity, and error buffers.
- [ ] Prove delayed acknowledgments cannot replace newer editor text.
- [ ] Prove operation/payload idempotency, changed-payload rejection, stale-version/lease rejection, definite-save error retention, and uncertain-outcome fencing/reconciliation.
- [ ] Carry project/document/session identity through callbacks and reject late callbacks.
- [ ] Prove process interruption after commit and before acknowledgment recovers the correct body.
- [ ] Read back WAL/FULL/foreign-key configuration in file-backed tests.
- [ ] Integrate persistence with the UI only after the core contract is complete; W2 does not complete the full V3 goal.

### W3 — library, free-order work, recovery, and A trial

**Gate:** M/P plus the development-native author trial. **Status:** planned.

- [ ] Implement New/Open/Rename/Duplicate/Archive/Locate and blank note/character/chapter creation.
- [ ] Persist last item/caret state, switch only after flush, and enforce project locks and captured ownership.
- [ ] Restore into a new recovered project with a new identity and active operation namespace; keep the original untouched on failure.
- [ ] Keep copied receipts historical and unable to authorize new operations.
- [ ] Add the explicit UTF-8 `Export draft` action from a flushed, frozen source.
- [ ] Complete the A trial across two offline projects, restart/resume, recovery copy, and draft export.
- [ ] Keep V2 import deferred to F1 and do not expose a placeholder Import V2 action.

### W4 — persistent conversation and deterministic jobs

**Gate:** M/P; no B trial yet. **Status:** planned.

- [ ] Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a deterministic mock provider.
- [ ] Cover delayed output, malformed structured output, partial failure, cancellation, Stop ordering, and exact repeatable suggestions.
- [ ] Keep provider code from mutating manuscript bodies.
- [ ] Preserve author-room/prose-context separation and safe-brief rules for deliberately transferred privileged material.
- [ ] Recover discussion and job state on reload and project switching; retry creates a linked new run.

### W5 — review cards, prepared snapshots, and single author Apply

**Gate:** M/P; B trial remains blocked until W6. **Status:** planned.

- [ ] Implement source-bound proposals, editable prepared versions, exact before/after preview, one-at-a-time Apply, and Reject.
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
- [ ] Cover streamed completion, refusal/truncation, authentication failure, broken/partial streams, Stop, process cleanup, and recovered terminal history.
- [ ] Qualify credential entry/storage and inspect logs/backups for leakage.
- [ ] Document opaque upstream retries and the limits of local idempotency; do not claim exactly-once external billing.
- [ ] Keep unqualified adapters unavailable and never silently substitute a provider/model.

### F1 — V2 migration

**Gate:** named migration evidence. **Status:** planned.

- [ ] Implement staged read-only import and reconciliation for the tested V2 schemas only.
- [ ] Preserve active semantics versus inert legacy evidence and retain the original V2 application/data.
- [ ] Do not present Import V2 before representative snapshots and reconciliation pass.

### F2 — reviewed story boundary

**Gate:** architecture scenario 5. **Status:** planned.

- [ ] Implement typed accepted rules/records, staged ready bundles, basis manifests, explicit exceptions, dependency links, and conservative suffix fences.
- [ ] Enable reviewed-source continuation and ready export only with explicit validity; do not imply exhaustive continuity.

### F3 — context quality

**Gate:** measured task-specific context evidence. **Status:** planned.

- [ ] Add source packing, author-room/prose-context separation, safe briefs, exact previous prose, aliases/search, and freshness checks.
- [ ] Measure omissions and permissions before claiming a memory or prompt improvement.

### F4 — narrative evaluation

**Gate:** independent author-labelled evaluation. **Status:** planned.

- [ ] Run retrieval, continuity, prose, and author-acceptance cases using frozen model/settings.
- [ ] Limit conclusions to the evaluated English tasks, genres, lengths, and models.

### F5 — batch Apply

**Gate:** B complete and separate atomicity evidence. **Status:** planned.

- [ ] Implement same-base disjoint preparation and one atomic Apply/decision transaction.
- [ ] Cover overlap, repeated operation IDs, stale/already-decided members, and lost acknowledgment.
- [ ] Preserve individual Apply as a single explicit author decision.

## Completion rule

The full V3 goal is complete only after the applicable W0–W8 and F1–F5 gates have their implementation, failure coverage, and evidence recorded. Current W1/W2 progress is necessary groundwork; it does not close the A writing trial, B feedback trial, C release qualification, live-provider qualification, migration, reviewed-story, context-quality, narrative-evaluation, or batch-Apply gates.
