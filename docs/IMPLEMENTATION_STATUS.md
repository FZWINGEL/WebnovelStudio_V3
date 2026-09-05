# V3 implementation status

**Status date:** 5 September 2026

**Current branch:** `codex/v3-persistence`
**Overall:** in progress; the full V3 goal is not complete.

The native development app supports persistent projects, free-order English writing, mock discussions, exact context inspection, adopted guidance, saved discussion sources, optional approved writing briefs, selected-passage Apply/Reject, saved-version restore, and frozen Markdown/TXT export previews. C0–C2 and parts of C3 are implemented. [CI run 33990073110](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990073110) for `16bf8bc` passed Windows/Ubuntu contracts and the strict 25-check native flow on WebView2 `151.0.4129.101`. The writing-brief checkpoint `508aee1` passed [CI run 33991463598](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33991463598), including Windows/Ubuntu contracts and all 26 strict native checks on WebView2 `151.0.4129.101`. The latest local diagnostic passed 26 of 27 checks with only the known local clipboard issue omitted. Production discussions remain mock-only. Installed-release, full V3, and narrative-quality gates remain open.

The Windows child-process foundation, incremental observer, and pure Claude/Codex JSONL parsers are implemented without a production dispatch connection. The current local slice adds bounded provider streaming and cleanup-aware Stop behavior, while the opt-in installed-package lifecycle remains under qualification. Release builds hide the session-only editor trial.

## Repository and CI evidence

- The private repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3). Its default branch is `main`, whose current tip is `d0eebfd780e435c068ef1017cac580786360d36b`.
- C3 saved-source checkpoint `16bf8bca2448cbf767886c38c6cc54d9e35c3131` passed [CI run 33990073110](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990073110), including both contract platforms and all 25 strict native checks. A later run of the same app source, [33990403189](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990403189) for the package-harness-only `1e6e556`, passed both contract jobs but failed the duplicate/restart prose assertion. Its harness killed the app after clicking All projects without waiting for asynchronous navigation/flush to finish. The corrected harness waits for the Library heading before killing the process. This is a supported race diagnosis, not a claim that the failed run passed. The corrected wait and writing-brief flow subsequently passed all 26 strict native checks in run 33991463598 for `508aee194fafa44a76afcd2885d42559d9d77853`.
- C2 implementation checkpoint `cd1d8a68ef3e55733a6252f8625da6812819a639` contains the C0–C2 implementation and the context inspector/source-pin preparation. Its [CI run 33975322590](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33975322590) completed successfully across Windows native, Windows, and Ubuntu contract jobs.
- W4 checkpoint `e92eef8a212312972609057c61d6667db8d27c5f` introduced persistent discussion. W5 checkpoint `a2a01632890a44efbe84bc526674fc9bf06d3d94` is pushed; [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728) passed Ubuntu and Windows contracts plus the strict 21-check Windows-native flow. Earlier `9161fc8` and [run 33979310784](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33979310784) remain historical native19 evidence. W6 `1a6beb76456c49f1da559d6dd7325dbb6d508e44` is pushed; [run 33982596342](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33982596342) passed both contract jobs and the native build, then failed waiting for keyboard Redo.
- W7 `78a9fde847fb5d25375603d055a997d3e43ef3ab` is pushed. [Run 33986475862](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33986475862) passed Windows/Ubuntu contracts, workspace Clippy/tests, and the native build. On WebView2 `151.0.4129.101`, the native flow passed all 22 checks through history restore, including delayed caret-save acknowledgment and keyboard Redo. It failed at export because the native dialog saved to its default `Chapter draft.md`, despite the helper reading back its requested filename. The failure capture showed the successful-export presentation, but did not qualify the chosen path. The subsequent correction passed as recorded below.
- [CI run 33988660050](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33988660050) for `78367cd` passed Windows/Ubuntu contracts and the strict 24-check native flow on WebView2 `151.0.4129.101`, including clipboard, history recovery, exact chosen-path export, and focus restoration. This is the completed W7 development qualification checkpoint, separate from the newer C3 changes and installed-package lifecycle.
- Historical [CI run 33973213684](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973213684) for `73db1bc` passed all three jobs. [CI run 33973390433](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973390433) for `6456d33` exposed the native duplicate-edit failure; its ProseMirror transaction wait fix is included in pushed `c2a5262` and subsequently passed; the newer W6 failure is recorded above.
- The previous [CI run 33971040177](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33971040177), for `b8d4cb8`, passed both contract jobs, Windows Clippy/tests, and native build; native smoke reached the UI but timed out on a CSS-hidden diagnostic label. The committed fix waits for attachment and checks the actual runtime through IPC.
- The historical [W0 qualification record](W0_QUALIFICATION.md) remains the record for that spike. Current implementation status and remaining gates are maintained here.

## Current work

| Area | Status | Evidence or boundary | Remaining |
| --- | --- | --- | --- |
| W0 native editor | Partial baseline retained | Explicit sample editor trial in the real Tauri/WebView2 window, Rust snapshot validation over IPC, shared fixtures, session-only feedback/replacement | English native author trial, minimum-window behavior, external Word paste, screen-reader use, native backup/export dialog journey, and broader qualification |
| W1 structural scope | Implemented; P foundation covered | Shared JS/Rust snapshot/scope fixtures, exact structural validator, mutation/Unicode/size refusal tests, and local/CI contract checks | Requalify extensions when new editor nodes or grants are added; W0/release trials remain separate |
| W2 project/session/save | In progress | Core/frontend sessions, receipts/reconciliation, default persistent UI wiring, stable create-retry operation/document/block IDs, empty-project `reconcileProject`, mounted-editor lease adoption, and generic lost-acknowledgment tests are implemented | Complete file-backed persistence/reconciliation evidence and the broader author-trial integration; no full W2/V3 completion claim |
| W3 library/transfer | Active work | Registry, transfer, schema-7 migration with durable pre-upgrade backup and schema-1/2/3 recovery, isolated recovered operation namespaces, historical-snapshot authority fencing, and persistent Library/Workspace flows are underway/partly wired; this W5 checkpoint adds schema-8 proposal storage | Complete library/recovery/A-trial contract and evidence |
| W4 persistent discussions | Active local slice | Rust-owned threads/messages/runs/drafts, selected scope grants, frozen packet receipts, deterministic mock output, durable sequence/terminal events, queued Stop sealing, running Stop intent with Rust cleanup settlement, retained partial messages, retry/reload recovery, and native FeedbackPanel/ContextInspector integration | Finish malformed-output/cancellation/error/UI coverage, then durable Apply and broader W4 evidence |
| C2 context packet compiler | Implemented and pushed; native development smoke covered | Rust pure deterministic compiler, all-source validation, exact target/instruction/scope/mandatory-pin preservation, full eligible text when it fits, stable whole-block prefix packing with explicit omissions otherwise, durable exact packet/options/hash receipts, and native IPC; 20 compiler, 9 receipt, and 6 context migration tests pass | Qualify provider tokenization separately; C3 remaining integration and W4 completion remain open |
| C3 author guidance | Partial local implementation | Chat or direct entry opens an editable author-confirmed instruction; immutable exact versions support Next request, This document, and This project scopes, with CAS/idempotent mutation receipts, source-epoch invalidation, recovery retention/fencing, exact mandatory AuthorRoom packet binding, separate guidance handles, and inspector display. Restricted writing excludes all current guidance; request use is consumed only with a successful persisted discussion start. Unchanged unsuccessful retries retain original active one-use guidance and durable composer mode. Schema-10 persistent AuthorRoom sources have exact required-source receipts, recovery retention, and uncertain-save controls. Schema 11 retains optional writing briefs with explicit approval and exact restricted-packet binding | Complete richer conversation selection and broader Apply integration; keep qualification evidence current |
| W5 proposal review and Apply | Implemented selected-passage slice; pushed and CI-covered | Schema-8 proposals, explicit intent, restricted context, immutable prepared versions/decisions, exact structural validation, atomic Apply/Reject, stale/replay fences, and mounted-editor handoff | Whole-chapter/block/manual-rebind work, broader B trial, and separately owned F5 batch Apply |
| W6 saved versions and restore | Implemented development slice | Bounded metadata paging, exact inert comparison, atomic whole-document restore, shared Apply/restore reconciliation, before/after retention, process-interruption and rollback evidence | Remaining lifecycle/renderer-loss combinations and B trial |
| W7 exports/package | Active development slice | Exact frozen Markdown/TXT preview, explicit native Save, schema-9 immutable export records, stable release data/NSIS configuration, and strict hosted native24 evidence for exact export and focus | Installer lifecycle and broader N gates |
| W8 provider foundation | Local checks pass; no live adapter enabled | Job containment, bounded stdin/output, incremental observer (20 integration tests and 3 module tests), typed partial failures, 12 pure Claude parser tests, and 13 pure Codex parser tests | Provider configuration, streaming integration, live token budgeting, complete isolation evidence, and W8/E3 acceptance |
| GitHub/CI | Historical writing-brief and contract jobs passed; newer reruns failed | `508aee1`, CI33991463598, strict native26, WebView2 `151.0.4129.101`; latest standard run 33992126659 failed an Ubuntu brief assertion and package run 33992126374 failed on an already-selected Chapter UIA lookup | Fresh reruns after the bounded harness fixes, installed lifecycle, and later W8 changes |

## Current local evidence

The final local wrapper passes rustfmt, workspace Clippy with `-D warnings`, **277 active Rust tests** (267 core and 10 desktop; one ignored subprocess target), TypeScript/Vite, and **178 frontend tests**. The final embedded Tauri build succeeds with a 629.05 KB bundle and the existing size warning. At `2026-09-05T21:29:29.901Z`, the native diagnostic passed 26 of 27 checks on WebView2 `152.0.4191.62`, omitting only clipboard. It includes a real SQLite fault, renderer reload, local retry saving the response, verification that no extra discussion run is created, and the visible recovery notice with its retry action. Physical DPI, screen-reader use, and author trials remain unqualified. The hosted run 33991463598 remains valid historical strict-26 evidence.

The earlier saved-source C3 wrapper check passed rustfmt, workspace Clippy with `-D warnings`, **230 active Rust tests** (229 core and one desktop; one ignored subprocess entry), TypeScript/Vite, and **162 frontend tests** on Windows 11 Pro `10.0.26200`. The bundle is 623.41 KB and retains Vite's size warning. Source-choice tests cover scope/CAS/replay, recovery, unavailable sources, mandatory overflow, stale packets, retry identity, restricted exclusion, uncertain acknowledgments, current-list refresh, keyboard focus, and late owners. A legacy receipt regression preserves exact packet input after adding the optional required-source annotation. The earlier release compile passed; final C3 installed-release qualification remains separate. Final receipt-validation guards passed the 12-test source-pin target, the 24-test transfer target, the 10-test context-packet target, and workspace Clippy. These add one regression after the full wrapper run, for 231 active Rust tests across the executed targets. The final embedded development build again passed the 24-check native diagnostic with clipboard omitted.

The new [Windows process contract](WINDOWS_PROCESS_CONTRACT.md) distinguishes explicit `finish_or_stop` cleanup from best-effort Drop. The incremental observer delivers only accepted bounded stream prefixes, including the normal final drain, and tests retain live child/grandchild handles, verify Job accounting reaches zero, reject incomplete zero-exit input delivery, and prove a previously requested Stop sends no packet bytes. Pending overlapped I/O retains stable owned storage; unresolved cancellation can retain that bounded allocation and detach readers rather than claim successful cleanup. The pure Claude and Codex parsers reject malformed/unknown/tool events, retain validated partial assistant text, and strip upstream diagnostic details. Their usage tests cover signed-negative conversion and bounded counters. These components do not enable a live provider or claim upstream isolation/billing guarantees.

The Rust Stop lifecycle now seals a queued discussion immediately, records a durable stopping intent for a running discussion, and lets the worker settle cleanup as stopped or interrupted. Retained partial output is included in the inspectable terminal message; failed claim/completion/Stop local writes remain owner-keyed app-memory state with a visible “Retry saving response” action, current-lease/reconciliation fencing, and no generation replay. This local contract is explained in [ADR 0009](ADR_0009_DISCUSSION_RECOVERY.md).

Earlier runs exposed an Ubuntu test synchronization issue, unsupported UIA focus, a native-dialog default-filename error, and return-focus loss. Their corrections are covered by the successful strict run 33988660050. Run 33988179660 passed contracts but was canceled as superseded before native completion; its cancellation is not a functional failure.

The earlier full W7 wrapper check, before the ordinary-period export correction and W8 process work, passed rustfmt, workspace Clippy with `-D warnings`, **186 active Rust tests** (185 core and one desktop; one ignored subprocess target), TypeScript/Vite, and **154 frontend tests**. That historical bundle was 615.31 KB.

W7 adds thirteen core transfer regressions, including exact projection/bytes, source tampering, frozen older revisions, no overwrite, duplicate/concurrent finalization, post-install begin/insert failure, immutable-record recovery, schema-8 migration, and Markdown punctuation/URL/indentation. The final transfer target passed all 24 tests after ordinary-period and Windows-basename fixes; the old direct TXT export IPC/core path was removed. Ten ExportDialog tests cover ownership, corrupt preview, explicit save/cancel, possible writes, existing destinations, and late results. Six background-caret tests keep typing available during delayed acknowledgments, defer stale views, fence cleanup, and make later lifecycle operations wait before flushing current writing. The release-profile Windows x64 binary also compiled successfully; the installer and native journey remain separately tracked below.

The unsigned Windows x64 NSIS package builds with the bundled offline WebView2 installer and stable release data path. Hosted package run 33990404236 built and installed `1e6e556`, opened the release Library, created synthetic prose, and read it back, then failed before returning to Library. Its owned-window capture showed an uncertain save. Source inspection identified `validate_snapshot` incorrectly guarded by `debug_assertions`, although production autosave calls it. The shared command registration is corrected. The rebuilt release executable passed owned UIAutomation create/write, saved status, Library return, and exact text retention after reopening in an isolated synthetic data root. The full installed lifecycle still needs a fresh hosted run. Normal close/uninstall/reinstall were not reached in that failed run and cleanup was forced. See [Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

W6 adds six core history tests, including cursor pagination through the final page, a six-stage rollback matrix, backup decision-link tampering, and recovered-project namespace behavior. A real process is killed after restore COMMIT and before acknowledgment; replay recovers the historical result once. Eight frontend restore tests and eight HistoryPanel tests cover the input/lifecycle barrier, exact preflight, lost acknowledgment, fenced absence, later-head conflict, failed display recovery, and late panel responses. Existing W5 proposal, Apply, scope, and history-boundary tests remain green.

The tracked C3 flow now has **26 checks**, including approved writing briefs; the final rebuilt local diagnostic passed **25** with only clipboard omitted. Checkpoint `508aee1` has a strict **26-check** hosted pass, including clipboard and writing briefs. The earlier saved-source checkpoint retains its separate strict **25-check** hosted pass. Native checks include real committed Apply/restore with a discarded acknowledgment, same-editor reconciliation, one-step undo/redo, exact export bytes, and retained history. Transport-loss injection lives only in the external harness.

The strict local native flow still stops at W0 Ctrl+V: copy serializes correctly, but paste receives empty clipboard data. This also occurred with an older binary. A separate Win32 diagnostic received access denied from `OpenClipboard(NULL)` in 20 of 20 attempts; no owner window was reported. The cause remains unresolved; no clipboard service was restarted and no clipboard contents were inspected. The successful strict W5 GitHub run is separate evidence on the hosted Windows machine. Ignored `.local/native-other-results/` explicitly records omitted clipboard coverage; `.local/native-results/` contains strict-run output. A stale failure file does not supersede a later dated success report.

The owned-dialog export helper uses `WM_NEXTDLGCTL`, `EM_SETSEL`, and `EM_REPLACESEL`, verifies exact readback, and sends no global filename keystrokes. ExportDialog closes before parent focus is restored. Both behaviors passed the strict hosted native flow; installed-package qualification is separate.

Default-size history comparison and restored-history screenshots were inspected. An additional 800×600 CSS-viewport check inside actual WebView2 shows no horizontal overflow and keeps comparison, Restore, and manuscript controls reachable. This is emulated viewport layout evidence, not physical window-resize, DPI, accessibility, or screen-reader qualification. Native dialogs, normal close, packaging, and broader author trials remain separately open.

Three bounded Codex CLI `0.153.3` generation dispatches are recorded in [Codex qualification](CODEX_QUALIFICATION.md): an initial success, a dedicated-home authentication failure, and a success using the existing managed login with stricter requested capability controls. No tool events were observed, and effective model/traits were not independently echoed. The empty-cwd, requested-control, host-authentication, and upstream-retry boundaries remain explicit. Descendant containment, Stop, failure, and context-budget qualification remain open; no live adapter is enabled in the application.

## Full completion checklist

C2 review added explicit scope requirements for prose-producing purposes, mandatory evidence-dependency closure, non-sensitive policy exclusion counts in the delivered envelope, and a 10,000-block regression for bounded packing memory. The context inspector is mounted in the persistent discussion surface, and source pins are captured. C3 now persists author-confirmed guidance and binds frozen typed guidance exactly into AuthorRoom packets; its mutation receipt is distinct from packet guidance handles. Recent author-room discussion context is implemented as a bounded recency selection: up to four complete delivered exchanges and 16 KiB of exact turn records from the same current document thread and policy. Frozen snapshots and receipts retain the exact messages and scopes; the inspector shows supplied exchanges and omissions separately from guidance and story evidence. Stopped or partial output, other documents, revoked-policy turns, and copied historical threads are excluded from automatic reuse. This is recency selection, not semantic retrieval or adopted story truth. See [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md). Preparing or reading a packet does not authorize dispatch: W4 atomically claims a run against current source/policy epochs before invoking the deterministic mock. An explicit linked retry of a stopped, failed, or interrupted discussion retains its exact original request-scoped guidance when those versions are still active. Feedback, selected scope, and ordered source pins must match the original request; editing any of them starts a new request. Current document/project guidance and current permitted story sources are compiled afresh. Newly waiting request guidance is reserved for the next new request. Edited or retired inherited instructions, revoked policies, completed runs, and recovered-copy links are refused. The schema-7 composer stores the retry link with its draft and immutable save receipt, so navigation/reload preserves the choice. The original guidance-use receipt remains the only consumption record; retries do not consume it again. Persistent document/project discussion sources are implemented with schema-10 CAS receipts and mandatory packet binding; optional approved writing briefs are implemented; live-provider integration remains open; the selected-passage Apply slice is implemented.

The following checklist preserves the approved work-package order and gates. A package is complete only when its implementation, failure coverage, and named evidence gate are recorded. W2 completion does not complete A, B, C, or the full V3 goal. W5 development CI is green. A checked implementation item does not by itself close a broader author-trial or release gate.

Persistent document/project source choices are implemented for AuthorRoom discussions. **Keep source…** opens an explicit confirmation form; **Include next time** remains a one-request choice. Rust merges current saved choices with transient pins into the frozen packet, retains exact mandatory-source receipts, and refuses unavailable or oversized required sources. The current target is already mandatory and is included once. Restricted edit requests exclude these saved discussion choices. Changed choices advance the source epoch, while retries preserve their original transient request identity. See [ADR 0007](ADR_0007_DISCUSSION_SOURCE_PINS.md).

### W0 — native editor spike and contract lock

**Gate:** initial N-spike evidence; E1 begins here. **Status:** partial/in progress.

- [x] Keep a real Tauri/WebView2 development window with the restricted Tiptap schema and persistent mounted editor instance.
- [x] Record the snapshot, identity, scope, canonicalization, hash, short local barrier, and session-history boundary in [ADR 0001](ADR_0001_EDITOR_CONTRACT.md).
- [x] Keep shared JS/Rust golden fixtures and real IPC snapshot validation.
- [x] Exercise session-only feedback, selection quotation/focus, preview/reject, local strict replacement, and undo/redo in the development surface.
- [ ] Complete the English native author trial, minimum-window/DPI behavior, external Word paste, native backup/export dialog journey, and assistive-technology trial.
- [x] Record a successful remote CI rerun after the native ProseMirror transaction wait fix and close the nominated Windows configuration evidence.
- [ ] Do not claim W0, N, an installed release, or real-manuscript readiness from the current spike alone.

### W1 — canonical documents and structural scope validation

**Gate:** P foundation. **Status:** implemented; P contract evidence recorded below.

The P foundation is covered by shared snapshot and scope fixtures, independent Rust validation, and file-backed persistence tests. `crates/core/src/lib.rs::shared_snapshot_fixtures_match`, `crates/core/tests/scope.rs::shared_scope_fixtures_match`, `scope.rs`/`text_replacement.rs` mutation tests, and frontend `document.test.ts`/`scope.test.ts` establish the current restricted schema. The full check recorded above passed, and W5 CI exercised the unchanged shared contract on Windows and Ubuntu. This qualifies the implemented document contract; new editor nodes or scope kinds require new evidence, and it does not close W0 or release trials.

- [x] Implement the restricted document schema and canonicalization/hash contract as shared JS/Rust behavior.
- [x] Implement block identity rules and Unicode endpoint conversion for the supported English editor surface.
- [x] Implement the independent Rust structural token iterator and scope validator.
- [x] Prove unchanged outside-scope text, marks, links, block style/identity, and scene boundaries.
- [x] Cover surrogate-pair, combining-mark, ZWJ, repeated-occurrence, empty-block, inline-only, cross-paragraph, malformed, and oversized fixtures as internal Unicode robustness cases.
- [x] Keep Rust free of a general ProseMirror-step interpreter.
- [x] Record P evidence before treating W1 as complete.

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
- [x] Read back WAL/FULL/foreign-key configuration in file-backed tests.
- [ ] Complete and qualify the persistence integration across the UI and author trial; W2 does not complete the full V3 goal.

### W3 — library, free-order work, recovery, and A trial

**Gate:** M/P plus the development-native author trial. **Status:** active work.

- [x] Implement New/Open/Rename/Duplicate/Archive/Locate and blank note/character/chapter creation.
- [x] Persist last item/caret state, switch only after flush, and enforce project locks and captured ownership.
- [x] Restore into a new recovered project with a new identity and isolated operation namespace; keep the original untouched on failure.
- [x] Keep copied receipts historical and unable to authorize new operations.
- [x] Add the explicit UTF-8 `Export draft` action from a flushed, frozen source.
- [ ] Complete the A trial across two offline projects, restart/resume, recovery copy, and draft export.
- [ ] Keep V2 import deferred to F1 and do not expose a placeholder Import V2 action.

### W4 — persistent conversation and deterministic jobs

**Gate:** M/P; no B trial yet. **Status:** active local slice.

Integrate the C0–C3 context foundation before or alongside this package: frozen snapshots and exact eligible sources, source epoch, deterministic exact retrieval with dirty-index fallback, mandatory-budget refusal, scoped author guidance, and actual-packet receipts/inspector. C0 and C1 are implemented in the core; C2's pure compiler, durable exact receipts, and native IPC are pushed and covered by local smoke; C3 guidance and bounded recent discussion compilation are pushed; linked retry guidance and durable composer mode are implemented and pushed. Persistent discussion sources and optional approved writing briefs are implemented; richer relevance selection remains open. These context packages do not replace the base save, Apply, lifecycle, or authority contracts.

- [x] Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a deterministic mock provider.
- [x] Implement queued Stop sealing, running Stop intent, cleanup settlement, retained partial output, and inspectable terminal recovery.
- [ ] Cover delayed output, malformed structured output, partial failure, cancellation, and exact repeatable suggestions.
- [ ] Keep provider code from mutating manuscript bodies.
- [x] Preserve author-room/prose-context separation and optional author-approved brief rules for deliberately transferred directions; see [ADR 0008](ADR_0008_WRITING_BRIEF.md).
- [x] Recover discussion and job state on reload and project switching; retry creates a linked new run.

### W5 — review cards, prepared snapshots, and single author Apply

**Gate:** M/P; B trial depends on W6. **Status:** selected-passage slice implemented and CI-covered; broader package remains open.

This W5 checkpoint adds explicit `Discuss`/`ProposeEdits` intent while retaining `Discuss` as the wire default. `ProposeEdits` currently requires a selected passage in a chapter and builds a `Working`/`RestrictedWriting`/`Revise` request at the chapter reader frontier; author-room private/future material, current guidance, and recent chat are excluded. Provider terminal handling accepts only a strict 1–3 candidate `ProposalOutput`; malformed or unsupported output is retained as raw unplaced discussion text without a repair call. The deterministic mock returns three alternatives.

Candidates, prepared versions, and decisions are immutable. Prepare accepts historical records against their immutable original source and validates exact JS result text, marks, and block scope with CAS versioning; only Apply requires the current head. Apply and Reject are explicit author actions. Apply atomically records body, source epoch, before/after revisions, decision, and receipt; any source edit or current-policy change makes a pending proposal stale. Reject leaves the epoch unchanged. Duplicate and cross-operation receipt collisions are fenced; Apply acknowledgment carries the separate latest result, and replay returns the latest head without reapplying. Recovered-copy proposals remain read-only.

The parent editor session flushes and preflights behind a pending-Apply barrier, commits the durable operation before dispatching the exact existing-editor transaction with `closeHistory`, retains newer edits on conflict, and reconciles uncertain acknowledgments without autosave. Native IPC mock commands are wired. Focused W5 tests and the diagnostic native subset pass locally; strict native21 passed in W5 GitHub CI. The local clipboard issue remains recorded above. Whole-chapter/block/manual-rebind proposals and F5 batch Apply remain open; W6 now owns explicit revision restore.

- [x] Implement source-bound proposals, editable prepared versions, exact before/after preview, one-at-a-time Apply, and Reject.
- [x] Bind each proposal to its context snapshot, exact target/scope, source epoch, policy, and context receipt; F2 alone owns reviewed authority.
- [x] Use the short local mutation barrier, preflighted editor transaction, durable decision/before/after/receipt transaction, and saved-generation handoff.
- [x] Add selection toolbar, context-menu action, and keyboard/menu alternative.
- [x] Prove undecided suggestions remain unchanged, repeated Apply cannot mutate twice, stale work is refused, and selected edits cannot change neighboring text, style, or boundaries.
- [x] Keep Apply all/batch Apply out of this package; F5 owns that later contract.

The checked items describe the selected-passage slice. Evidence includes `crates/core/tests/proposals.rs` preparation, scope, replay, rollback, recovery, and tamper cases; frontend `apply.test.ts` and `ProposalPanel.test.tsx`; `Writer.tsx` selection entry points; and the W5 strict native CI result. Whole-chapter/block/manual-rebind behavior and the broader B/E4 trial remain open.

### W6 — lost acknowledgment, shared lifecycle, history, and interruption hardening

**Gate:** P with native reruns; closes the B trial gate. **Status:** history/restore and shared pending-change reconciliation implemented; remaining trial and interruption combinations open.

See [ADR 0005](ADR_0005_DOCUMENT_HISTORY.md). Restore preserves the current writing in a checkpoint and advances the story source epoch. The operation receipt is its immutable author decision; schema 8 needs no new decision table.

- [x] Reconcile pending operation IDs and latest heads after lost acknowledgment.
- [ ] Share one lifecycle guard across Apply, reconciliation, editor disposal, switching, close, and application-controlled reload.
- [x] Add in-session history boundaries, significant undo/redo checkpoints, restart comparison, and explicit restore.
- [ ] Cover forced renderer loss, process interruption, restore A while B runs, and old-or-new transaction outcomes.
- [ ] Include context snapshots, source epoch, policy, delivered packet, and receipt in restart/fence/Stop coverage; no late context operation may trigger an implicit paid retry.
- [ ] Retain the live buffer on disk-full and permission errors; never hide external retries or paid restarts.
- [ ] Run the E4 stale-proposal friction check and keep conservative staleness unless measured evidence supports a bounded alternative.
- [ ] Run the B feedback trial only after the durable Apply and lifecycle evidence is complete.

### W7 — explicit exports and packaged native qualification

**Gate:** M/P/N complete. **Status:** export implementation and package build present; full native/release qualification remains open.

- [x] Add Markdown beside draft TXT with an exact frozen single-document preview, formatting/omission disclosure, immutable source record, and explicit working-draft export. Multi-document and reviewed-ready export remain separately gated.
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

The maintained [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) and [first-slice plan](V3_STORY_CONTEXT_FIRST_SLICE.md) are an adopted design extension. Their C0–C6 packages are part of the full V3 goal and preserve the base save, Apply, lifecycle, and reviewed-authority ownership. C0 is implemented with pure contracts and 16 adversarial tests; C1 is implemented as a working-basis snapshot/retrieval slice; C2 is implemented and pushed as a Rust pure deterministic compiler with durable exact packet receipts and native IPC, and is covered by the current native development flow and earlier hosted checkpoints; C3 guidance persistence and packet binding are pushed, as are bounded recent discussion context and inspector display. Linked retry guidance and saved composer mode are implemented and pushed. Persistent discussion sources and optional approved writing briefs are implemented; richer relevance selection remains open. No full C0–C6 completion is claimed.

| Package | Planned owner and scope | Status | Required evidence before completion |
| --- | --- | --- | --- |
| C0 | Before/alongside W4; freeze contracts and adversarial eligibility fixtures | Implemented | Pure contracts and 16 adversarial tests cover source eligibility, disclosure boundaries, digest restrictions, and authority separation |
| C1 | Before/alongside W4; immutable source snapshots, exact retrieval, source epoch, and dirty-index fallback | Implemented working basis | Rust actor snapshots pin canonical revisions; exact literal/lexical scan, source-only alias matches, Unicode UTF-16 spans, revocation epoch, conservative source staleness, snapshot retry/restart, and yielding disposable per-document index rebuild. Ten C1 tests include a 1,000-chapter, 8,280,000-byte cold snapshot measured at 1.1 seconds in local debug; six context migration tests cover schema-2/3 upgrade and recovery, with transfer coverage for schema-1 recovery. Reviewed/history/character policies remain unavailable until authority work; aliases remain private to AuthorRoom until safe grants, and AuthorRoom Revise/Continue is blocked |
| C2 | Before/alongside W4; deterministic multi-resolution packet compilation, mandatory-budget errors, and actual-packet receipts | Implemented and pushed; development smoke covered | Pure compiler and durable receipt tests pass; exact packet/messages/options/hash survive restart; source body/descriptor/projection/eligibility/scope validation, target/instruction/scope/mandatory-pin preservation, full-eligible-when-fitting and whole-block-prefix packing with explicit omissions are implemented. Mock accounting is UTF-8-byte based only; provider tokenization, live AI integration, and release qualification remain open |
| C3 | Before/alongside W4; scoped author guidance and the context inspector | Partial: guidance persistence, bounded recent exchanges, inspector, transient pins, persistent discussion sources, and approved writing briefs integrated | Chat or direct entry can be saved, edited, and retired as immutable exact versions at Next request, This document, or This project scope. CAS/idempotent guidance receipts, source-epoch invalidation, recovery retention/fencing, exact mandatory AuthorRoom packet binding, one-use consumption after successful persisted start, separate guidance handles in the inspector, and GuidancePanel lost-ack/late-response coverage are covered locally. Recent complete exchanges are frozen and packed with exact message receipts and explicit omissions; stopped/partial, other-document, revoked-policy, and copied historical turns are excluded. Unchanged unsuccessful retries preserve original one-use instructions without consuming newly waiting guidance; request identity, current policy, active versions, restart, and recovered-copy boundaries are tested. Optional approved briefs preserve exact restricted request text without transferring private origin material. Richer conversation selection and broader Apply integration remain open |
| C4 | F3; source-bound generated digests and richer quality without automatic canon | Planned | Rebuild, late-result, source-change, deletion, restore, and citation-range evidence; no paid autosave analysis |
| C5 | F3 after F2; thin temporal, relationship, and thread views | Planned | Reviewed-authority dependency, source-bound view rebuild, and quality evidence for the supported English tasks |
| C6 | W8 additional qualification; bounded provider-side read loop | Planned | Stop/budget/duplicate-event/crash boundaries, visible unknown outcomes, and fresh invocation labeling |

The public promise is layered: stored evidence, permitted available sources, the packet actually delivered, and what a model understood are separate states; the last requires evaluation. C1 retains original source and does not make copied historical snapshots authoritative for a new project. Context work does not authorize automatic canon or replacement of source text with a large rolling summary.

## Completion rule

The full V3 goal is complete only after the applicable W0–W8, F1–F5, and C0–C6 gates have their implementation, failure coverage, and evidence recorded. Current W1/W2 progress and active W3 work are necessary groundwork; they do not close the A writing trial, B feedback trial, C release qualification, live-provider qualification, migration, reviewed-story, context-quality, narrative-evaluation, context-extension, or batch-Apply gates.
