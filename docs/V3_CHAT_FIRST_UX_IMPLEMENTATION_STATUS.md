# Chat-first implementation status

This record describes the current implementation of the [chat-first
specification](../V3_CHAT_FIRST_UX_SPEC.md) and [implementation
plan](../V3_CHAT_FIRST_UX_IMPLEMENTATION_PLAN.md). The architecture decision is
recorded in [ADR 0034](ADR_0034_PROJECT_CONVERSATION.md). These documents remain
the product contract; this file records what is implemented and what still
needs qualification.

## Current state — 9 September 2026

### Completion audit follow-up

The second requirement audit found remaining review details beyond the earlier
checkpoint. Assumption corrections can now be edited directly and staged in
the unsent project composer. A separate Send remains necessary, and staging
refuses an active restricted chapter task. Saved response decisions display
their recorded scope, audience, rationale and version; historical events retain
their own values, with reversal offered only for the latest disposition.

Chapter suggestion review now displays the frozen source version, editable
scope, and protected surrounding prose with its formatting. A deterministic
return recap projects recent ordinary save receipts and can inspect an exact
retained checkpoint. It creates no transcript messages or model requests.
The complete frontend suite passes **701 tests across 74 files**
(`.local/chat-audit-frontend.log`). The receipt recap's Rust reopen/isolation
regression also passes. The integrated native build and current qualification
are recorded below.

The final complete local gate passes **920 Rust tests** (122 core, 710 grouped
integration, 88 desktop; one additional test intentionally ignored), **701
frontend tests across 74 files**, and **25 tooling checks**, plus formatting,
strict Clippy, TypeScript, and the production build. Log:
`.local/chat-audit-final-check.log`. The only build warning is the existing
large frontend chunk warning. The first full run exposed an optional
app-server readiness race: the driver acknowledged startup before publishing
its Ready state. Both handshake paths now publish Ready first, with a
deterministic ordering regression. The final concurrent suite passes; the
failed run is retained at `.local/chat-audit-readiness-failure.log`.

Grouped review now includes a versioned effects manifest with exact ordinary
endpoint heads, relationship dependencies, and protected content. Rust derives
the proposed relationships from the retained provider response; the renderer
cannot substitute a different group. Up to three drafts from that response,
their new or updated ordinary documents, supported relationships, decisions,
and one source-epoch advance commit in the same transaction. Six focused tests
cover exact heads, excluded endpoints, stale dependencies, replay, unsupported
effects, and an injected SQL failure after document writes followed by a local
retry. New requests use `project-chat-prompt.v3`; legacy and v2 packet bytes
remain unchanged. Nonempty impacts, supersessions, and placements are refused
before preview. Automatic organization and inferred semantic synchronization
are outside this slice.

The rebuilt native harness passed **17 checks in 19.647 seconds**, including
the exact saved-document recap, explicit draft rejection, and request-scoped
**Not now** followed by a fresh explicit request. The Writer suite passed
**52 checks in 92.933 seconds**. The grouped live Codex Exec trial passed
**24 assertions across one request in 25.030 seconds**, including visible
relationship review, atomic two-document adoption, and retained relationships
and exact committed heads after native restart. Its preceding one-request
attempt completed generation and adoption but exposed a harness assertion
using `relationshipType` instead of the persisted `type`; that test error is
corrected and its original evidence retained.

These three runs used the 3.0.0 development executable, **54,642,688 bytes**,
SHA-256 **`2838910af9ffe81fb27a9e4ace2c37ffe300695734184604301fae95de9fdc2d`**,
and WebView2 **152.0.4191.66**. Build log: `.local/chat-audit-native-build.log`.
The live request selected Luna/xhigh/priority, recorded installed Codex
**0.153.4**, and left effective model/tier unreported. It qualifies this bounded
Exec contract, not narrative quality or the other transports. Launcher records:

- Chat: `.local/isolated-native-launch/20260909-023916103-41520/launch.json`.
- Writer: `.local/isolated-native-launch/20260909-024159707-33496/launch.json`.
- Grouped live: `.local/isolated-native-launch/20260909-024100776-8124/launch.json`.

Each launcher confirmed cleanup of its owned isolated processes. Actual native
200% zoom geometry and the saved-document recap were also inspected on this
build. All projects were synthetic; the author's running workspace was not
used for qualification.

The final rebuilt executable includes the app-server readiness correction:
`target/debug/webnovel-desktop.exe`, **54,644,224 bytes**, SHA-256
**`79e5232b90f532b505fb109045d5fbf666f9db3dd10f3e459919ff506256f0f4`**.
Build log: `.local/chat-audit-final-build.log`. The Writer and live-grouped
reports above retain their earlier executable identity; the only subsequent
production change is readiness publication in the optional app-server runtime.
The Exec adapter, request recipe, editor, and adoption code are unchanged
between those builds.

Implementation checkpoint: **`8bbef1a720a381635b82ab86543095ec778c5092`**.
The final local gate and executable were built from the working tree recorded
by that commit. This checkpoint includes the readiness fix; subsequent edits
that record its identity are documentation only. The unrelated research note
under `docs/research/` remains excluded. No hosted CI or installer result is
claimed for this implementation commit.

On the final executable, native chat again passes **17 checks in 20.173
seconds** with confirmed owned-process cleanup. Report:
`.local/native-results/chat/report.json`; launcher:
`.local/isolated-native-launch/20260909-024821575-41792/launch.json`.

The installed-package harness now includes unsent chat composer retention
through normal close/reopen and same-version reinstall. Its PowerShell syntax
check passes; these new installed steps have **not been executed**. Existing
fail-closed runner and author-data checks remain intact. Same-version retention
does not qualify an upgrade. The latest inspected hosted CI attempt,
`34274285964` at `6ab1505`, never started its runner jobs because GitHub reported
an account payment/spending-limit admission failure. That historical attempt
does not establish the current billing state or qualify this working tree.

### Previous integrated checkpoint

The chat-first project conversation is implemented as an **opt-in native
development surface**. The default workspace remains unchanged while native
and formative evidence is collected. A project conversation can handle normal
project questions and chapter feedback in the same conversation. Chapter tasks
retain their exact chapter and selected scope; they do not receive the private
project transcript or assistant drafts implicitly.

The implemented review path keeps assistant output isolated as task drafts.
Drafts are inspectable and editable, then require an explicit review and
adoption action. Up to three nonchapter targets can be adopted as one grouped
operation with exact before/after previews and all-or-nothing persistence.
Question and assumption dispositions are scoped decisions, and conversation
history is available as a read-only historical surface. The current review
inventory is complete rather than capped at an arbitrary first page, and older
timeline items load through paging. A stale draft can be refreshed only through
an explicit assistant action; its unchanged earlier adoption preview remains
blocked after a source change. These features do not turn an assistant
suggestion into story truth without author adoption.

The completion audit added direct exact-source attachment from the document
panel, a proposed chapter handoff with explicit target selection, approved
writing brief and separate Send, and a keyboard-operable project panel resizer.
Review now leads with a change summary, the original request and assumptions,
affected documents, complete before/after text, and a deterministic diff. Old
draft origins load through conversation paging. Stale previews offer an explicit
comparison of saved targets and source/policy epochs; comparison cannot rebase
a stale draft or renew its authority.

The native journey exposed two brief-provenance defects: the versioned project
origin also entered the legacy chapter-thread check, and continuation's derived
append scope was compared with the author's null input scope. Both are repaired
with native-shaped regressions. Additional fixes fence late project callbacks,
cancel outdated asynchronous brief approvals, target local retries to the exact
retained run, restore detached editors through fenced reconciliation, and make
mobile Documents/Chapter navigation leave review mode reliably. Prompt recipe
`project-chat-prompt.v2` is recorded on new requests; absent historical versions
retain the exact legacy instructions and unknown versions are refused.

The final workflow pass adds provider-free **Bring a note** entry, a recent
project picker with verified activity and pending-draft badges, and persistent
document-panel search and selection per project. Source links independently
show the exact **Version discussed** and the **Current version** without
changing the frozen request. Successful ordinary saves refresh the visible
source-freshness status, including after a response has completed. Request
status distinguishes the frozen model/settings from the next picker choice.

Chapter-wide discussion can now propose one exact contiguous paragraph range
through `chapter-discussion-output.v1`. Rust validates the frozen head, block
identity, complete quotation, delivery status, and source/policy freshness.
**Use this passage for an edit** flushes pending typing, revalidates the same
result and current chapter, then stages a separate scoped request. It never
sends that request or changes prose. Older response contracts retain their
historical behavior. Shared provider-capability checks apply before either
new chat contract is accepted.

The current project reader floor is **schema 40**. Schema 39 adds document
roles and schema 40 adds the per-project conversation, immutable items, and
assistant-draft provenance. Existing documents and historical records retain
their identities and bytes.

## Work-package status

| Work package | Current state |
| --- | --- |
| CF0 persistence and role boundaries | Implemented through schema 40; current transfer and role validation is covered by the Rust/frontend suites. |
| CF1 project conversation and composer | Implemented, including persistent composer CAS, frozen request context, bounded output, and reopen/reconciliation paths. |
| CF2 isolated drafts and adoption | Implemented, including exact previews, explicit single/grouped adoption, scoped dispositions, stale checks, and local recovery. |
| CF3 chat/document shell | Implemented as an opt-in surface with direct note entry, recent-project activity, source attachment, persistent panel state, conversation/document resizing, original-request review, diff, original/current source comparison, pinned status, and responsive navigation. |
| CF4 native provider and recovery path | Implemented with deterministic native recovery coverage and a current two-request Codex Exec trial for handoff and chapter-range contracts. Installed-package and broader live qualification remain open. |
| CF5 chapter feedback and brief provenance | Implemented for same-conversation chapter requests, selected scope, confirmed assistant-suggested paragraphs, and the full proposed handoff → explicit target → approved brief → separate Send → chapter proposal path. Private project chat stays excluded. |
| CF6 qualification and rollout | In progress. Default-on rollout, human formative evaluation, accessibility, broader live-provider support, and installed-release gates remain open. |

## Earlier checkpoint evidence

The following evidence predates the audit follow-up above. It remains useful
for the recorded contracts and executable identities, but is not a substitute
for qualification of later source changes.

- The complete local gate exited 0 with **25 tooling checks, 121 core Rust tests, 703 integration Rust tests, 88 native-unit tests, and 683 frontend tests** across 72 frontend files, together with formatting, strict Clippy, TypeScript, and the production build. This is **912 passing Rust tests**, with one additional test ignored. Log: `.local/chat-completion-final-check.log`. The full frontend suite also passed after the initial layout repair (`.local/chat-layout-frontend.log`); subsequent CSS-only compact-layout corrections were verified by the rebuilt native smoke. The final spike build passes (`.local/chat-layout-final-build.log`).
- The rebuilt native chat smoke passes **16 checks in 18.816 seconds** with the deterministic local provider and confirmed isolated-process cleanup. It covers lost start acknowledgment, failed local materialization recovery, lost two-document adoption acknowledgment with one receipt and epoch advance, selected scope, recent-project switching and badges, provider-free note entry, original/current source inspection, actual 800×600 resizing, guarded WM_CLOSE/restart with unsent typing, keyboard panel resizing, exact source attachment, confirmed paragraph-range staging, and the complete approved chapter handoff without private-chat leakage or automatic prose changes. Report: `.local/native-results/chat/report.json`; launcher: `.local/isolated-native-launch/20260909-015019984-42768/launch.json`.
- Actual WebView2 **200% controller zoom** at the 800×600 native window produces a 400×300 CSS viewport. The status and composer remain in bounds, with zero page overflow and keyboard access to Send. The transcript retains a 55px visible region, its new-reply button stays entirely inside that region, and active Stop is reachable without scrolling request details. The root inspected an owned-window capture at `.local/native-results/chat/native-zoom-window.png`; geometry and active Stop measurements are adjacent JSON reports. CDP screenshots alone crop this zoomed WebView2 surface. This qualifies the measured development geometry, not screen-reader or physical author input.
- The rebuilt Writer-focused native run passes **52 checks in 92.425 seconds**, with no reported errors and confirmed isolated-process cleanup. Report: `.local/native-results/report.json`, dated 9 September at 01:52 UTC; launcher: `.local/isolated-native-launch/20260909-015122842-19664/launch.json`. Existing project-menu and title selectors now distinguish the new recent-project picker from the original controls.
- Both current native runs use WebView2 **152.0.4191.66** and the **3.0.0** development executable at `target/debug/webnovel-desktop.exe`, **53,868,032 bytes**, SHA-256 **`4fc9675197f73c4831ffe79a3019d91b7b73ca34ddea7adda850e7c2736d55d3`**. It was built from the working tree based on `06a8c38`, then recorded with the implementation checkpoint **`fe5eec18cc188dcf649e04defadc539d8cd4147b`**. The subsequent live trial records that checkpoint with a clean tracked tree; the unrelated untracked research note remains excluded. This is not a hosted CI or installed-release claim.
- The earlier live Codex Exec trial passed **27 assertions across exactly two requests in 33.258 seconds** on artifact SHA `9cf23071e708f11def5018ac44c3b875333998e27bdd20b5e63446b20fa12e72`. Each request produced an isolated draft, and the follow-up recalled `Mei` without repeating the name in its instruction. It requested Luna/xhigh/priority through `codex-stdin.author.v1`; the CLI reported `0.153.4`, cleanup settled, and effective model/tier remained unreported. Report: `.local/native-results/chat-live/report.json`. This is historical evidence for the legacy project-chat prompt, **not live qualification of the new optional handoff recipe**.
- The current Codex Exec contract trial passes **24 assertions across exactly two requests in 27.716 seconds**, on checkpoint `fe5eec1` and the same executable hash as the current native runs. Request one returns a `project-chat-prompt.v2` chapter handoff with no automatic chapter creation, adoption, or second call. Request two returns `chapter-discussion-output.v1` with an exact first-paragraph range. The getter-backed native review confirms and stages that range without a third request or changed ordinary heads. It requested Luna/xhigh/priority through `codex-stdin.author.v1`, observed CLI `0.153.4`, and retained both packets, outputs, and provider receipts. Effective model/tier were unreported. Report: `.local/native-results/chat-live-contracts/report.json`; confirmed isolated cleanup: `.local/isolated-native-launch/20260909-015532159-27084/launch.json`. This is a bounded response-contract trial, not a narrative-quality benchmark or app-server/Claude/HTTP qualification.
- The current live trial used only those two explicit requests, with no automatic retry or fallback. Deterministic native regressions used synthetic projects and the local test model. Failed development runs were corrected before final passing runs; intermediate reports are not final qualification.

These counts are development evidence. They do not qualify broad live-provider
coverage or narrative quality, installed-package behavior, screen-reader
behavior, or a human author trial. English-only authoring means
IME qualification is not a product requirement. The chat-first surface remains
opt-in until formative user evidence and the remaining release/accessibility
gates are deliberately run and reviewed.

The [author trial protocol](CHAT_FIRST_AUTHOR_TRIAL.md) is prepared but
unexecuted. It defines matched Workshop/chat tasks across five entry paths,
approximately 8–10 counterbalanced participants, next-day return, and separate
accessibility observations. The current installed-release data directory
contains author data, so the fail-closed installed-package harness must run in
a clean Windows account/VM or hosted runner; it must not be bypassed by moving
that data or spoofing CI flags. These external qualification gates keep CF6 and
default rollout open. The entire specification is not declared complete.

## Retained boundaries

Codex remains unpinned because the installed CLI is expected to update. Exec
remains the default author transport; optional app-server, Claude, and
OpenAI-compatible HTTP adapters remain separate integrations. The live trial
records the requested service tier but does not claim Fast/effective-tier
delivery when the provider reports it as unknown. Maintenance and summary
calls retain their dedicated GPT-6 Astra/low profile and do not inherit the
author-facing picker.

Runtime reuse does not imply upstream conversation-history reuse. Rust still
owns project identity, source permissions, frozen packets, story truth,
reconciliation, and explicit Apply/adoption. A local save retry never launches
another generation, and an uncertain acknowledgment keeps the original
operation identity for reconciliation.
