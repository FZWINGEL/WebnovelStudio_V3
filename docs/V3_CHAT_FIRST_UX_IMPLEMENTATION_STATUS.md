# Chat-first implementation status

This record describes the current implementation of the [chat-first
specification](../V3_CHAT_FIRST_UX_SPEC.md) and [implementation
plan](../V3_CHAT_FIRST_UX_IMPLEMENTATION_PLAN.md). The architecture decision is
recorded in [ADR 0034](ADR_0034_PROJECT_CONVERSATION.md). These documents remain
the product contract; this file records what is implemented and what still
needs qualification.

## Current state — 9 September 2026

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
| CF4 native provider and recovery path | Implemented with a deterministic native chat smoke covering recovery and the current workflows. The earlier two-request Codex Exec trial is historical evidence for the legacy recipe. Installed-package and broader live qualification remain open. |
| CF5 chapter feedback and brief provenance | Implemented for same-conversation chapter requests, selected scope, confirmed assistant-suggested paragraphs, and the full proposed handoff → explicit target → approved brief → separate Send → chapter proposal path. Private project chat stays excluded. |
| CF6 qualification and rollout | In progress. Default-on rollout, human formative evaluation, accessibility, live-provider, and installed-release gates remain open. |

## Verified evidence

- The complete local gate exited 0 with **25 tooling checks, 121 core Rust tests, 703 integration Rust tests, 88 native-unit tests, and 683 frontend tests** across 72 frontend files, together with formatting, strict Clippy, TypeScript, and the production build. This is **912 passing Rust tests**, with one additional test ignored. Log: `.local/chat-completion-final-check.log`. The full frontend suite also passed after the initial layout repair (`.local/chat-layout-frontend.log`); subsequent CSS-only compact-layout corrections were verified by the rebuilt native smoke. The final spike build passes (`.local/chat-layout-final-build.log`).
- The rebuilt native chat smoke passes **16 checks in 18.816 seconds** with the deterministic local provider and confirmed isolated-process cleanup. It covers lost start acknowledgment, failed local materialization recovery, lost two-document adoption acknowledgment with one receipt and epoch advance, selected scope, recent-project switching and badges, provider-free note entry, original/current source inspection, actual 800×600 resizing, guarded WM_CLOSE/restart with unsent typing, keyboard panel resizing, exact source attachment, confirmed paragraph-range staging, and the complete approved chapter handoff without private-chat leakage or automatic prose changes. Report: `.local/native-results/chat/report.json`; launcher: `.local/isolated-native-launch/20260909-015019984-42768/launch.json`.
- Actual WebView2 **200% controller zoom** at the 800×600 native window produces a 400×300 CSS viewport. The status and composer remain in bounds, with zero page overflow and keyboard access to Send. The transcript retains a 55px visible region, its new-reply button stays entirely inside that region, and active Stop is reachable without scrolling request details. The root inspected an owned-window capture at `.local/native-results/chat/native-zoom-window.png`; geometry and active Stop measurements are adjacent JSON reports. CDP screenshots alone crop this zoomed WebView2 surface. This qualifies the measured development geometry, not screen-reader or physical author input.
- The rebuilt Writer-focused native run passes **52 checks in 92.425 seconds**, with no reported errors and confirmed isolated-process cleanup. Report: `.local/native-results/report.json`, dated 9 September at 01:52 UTC; launcher: `.local/isolated-native-launch/20260909-015122842-19664/launch.json`. Existing project-menu and title selectors now distinguish the new recent-project picker from the original controls.
- Both current native runs use WebView2 **152.0.4191.66** and the **3.0.0** development executable at `target/debug/webnovel-desktop.exe`, **53,868,032 bytes**, SHA-256 **`4fc9675197f73c4831ffe79a3019d91b7b73ca34ddea7adda850e7c2736d55d3`**. It was built from the modified working tree based on `06a8c38`; this record does not claim a clean commit, current hosted CI run, or installed release.
- The earlier live Codex Exec trial passed **27 assertions across exactly two requests in 33.258 seconds** on artifact SHA `9cf23071e708f11def5018ac44c3b875333998e27bdd20b5e63446b20fa12e72`. Each request produced an isolated draft, and the follow-up recalled `Mei` without repeating the name in its instruction. It requested Luna/xhigh/priority through `codex-stdin.author.v1`; the CLI reported `0.153.4`, cleanup settled, and effective model/tier remained unreported. Report: `.local/native-results/chat-live/report.json`. This is historical evidence for the legacy project-chat prompt, **not live qualification of the new optional handoff recipe**.
- No additional live calls, automatic provider retries, or fallback were used during the completion audit. Native regressions used synthetic projects and the explicit local test model. Failed development runs were corrected before the final passing run; their intermediate reports are not final qualification.

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
