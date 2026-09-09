# Project conversation and isolated assistant drafts

The accepted [chat-first product specification](../V3_CHAT_FIRST_UX_SPEC.md) and
[implementation plan](../V3_CHAT_FIRST_UX_IMPLEMENTATION_PLAN.md) extend the
existing Rust/Tauri application. The implementation and qualification ledger
is [here](V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md). The input documents name
schema 38 as their baseline; the additive implementation uses schemas 39 and 40.

## Authority and storage

Schema 39 gives each document an immutable `ordinary`, `assistantDraft`, or
`conversationAnchor` role and raises the reader floor. Ordinary remains the
default and is omitted from historical-compatible document JSON. Generic
document operations, source enumeration, pins, reviewed evidence, memory,
lookup, chapter order, and manuscript export reject the other roles. A role
cannot be changed to turn an assistant draft into accepted story material.

Schema 40 adds one project conversation per project/operation namespace,
immutable conversation items, and assistant-draft provenance. Existing
discussion messages, runs, packets, revisions, and command receipts remain
authoritative. The blank anchor is only the required discussion target;
neither its name nor an ID prefix grants it special authority. Transfer
validation authenticates its role, blank body, ownership, and references.

The workspace project picker is a read-only projection of the library catalog.
It sorts active entries by `lastOpened` for recency, but that field is not an
activity claim. The native `project_activity` command returns counts only for
project actors that are already open; each snapshot carries the project ID and
operation namespace. The workspace accepts an exact current-project namespace
and only known open project IDs, so unopened or unavailable projects show no
activity or draft badge. Opening a picker entry is the only action that opens a
project; polling never acquires a lease or contacts a provider. Selecting an
entry closes the picker before navigation, and Escape restores focus to its
summary.

An empty project conversation can create **Bring a note** through the ordinary
document-create path. It creates an empty ordinary note with no provider
request, then opens it through the normal guarded session flow. Attaching that
note to a later conversation request still records the exact saved head.

The composer has compare-and-swap versions. Request acceptance stores the
exact composer, frozen packet, run, and conversation link in one transaction,
then clears that accepted composer version. Later typing stays in the live
composer. Autosave acknowledgments do not replace newer input. Conversation
bookkeeping and isolated draft saves do not advance the ordinary story epoch.

## Generation and conversation history

Root chat uses the existing Discuss lifecycle with explicit frozen metadata.
`project-assistant-output.v1` permits readable answers, up to two questions,
three assumptions, and three nonchapter drafts. The provider supplies typed
blocks and permitted handles; Rust allocates document and block identities.
Invalid or incomplete output remains inspectable but creates no adoptable
draft. Materialization is an idempotent local operation on retained completed
output, including after a restart. It never launches a repair generation.

Request callbacks are leased to the active project, operation namespace, and
conversation identity. Disposed requests and late polling/reconciliation
callbacks cannot mutate a newly opened project. A retry of retained work uses
only the exact retained run and operation identity; recovery never silently
dispatches a new generation. A definite pre-accept start failure clears the
active request and leaves the composer available for an explicit new **Send**.
An uncertain start remains pending and exposes **Reconcile** instead of
resending the request.

Native provider admission, model/effort/tier binding, Stop, output limits,
terminal settlement, and local-save recovery are shared with ordinary author
requests. There is one active root-or-chapter chat request per project.
Codex Exec stays the default; optional app-server requests still use fresh
upstream threads. Claude and compatible HTTP remain independent adapters.
Story Memory maintenance remains Astra/low and does not inherit the author
picker. No automatic memory summary or provider-native tool loop is added.

New project-chat packets record the frozen prompt recipe
`project-chat-prompt.v3` in their accepted metadata. The grouped recipe adds
the optional retained-output effects vocabulary while preserving the exact
bytes selected by legacy v1 and v2 packets. A missing recipe version selects
the exact legacy v1 prompt bytes so historical packets remain reproducible; an
explicit v2 value selects its original recipe; any unknown explicit version
fails closed. This is a packet-selection rule only. It does not rewrite
historical packet bytes or change the response envelope version.

Project conversation context selects at most four eligible complete turns
within 16 KiB and the existing provider packet allowance. It includes only
completed, delivered AuthorRoom evidence from the same current namespace.
Rejected/superseded candidate turns are omitted. The inspector distinguishes
prepared and delivered context; local history retention is not a claim that
the provider received the whole conversation or novel.

Question dispositions refer to exact producing output. Not now, Not relevant,
Keep mysterious, and Reconsider do not block unrelated requests. Applicable
scope and optional unknown-to audience are frozen as explicit decisions.
Assistant assumptions remain provisional and can be rejected. An explicitly
attached draft is labeled unadopted task material; a model revision creates a
new candidate with predecessor provenance and leaves the edited predecessor
intact.

An assumption correction is an author-room convenience: **Edit assumption**
stages an explicit correction, including the original and revised text, in the
current unsent composer. It never mutates the immutable response or draft and
never dispatches by itself; the author must press **Send**. A chapter task or
approved restricted brief refuses this author-room staging until the author
returns to project conversation. Disposition history keeps each event's own
scope, unknown-to audience, rationale, and payload version; later decisions do
not rewrite the values shown for an earlier event.

## Review and adoption

Preparing a review checkpoints the latest saved draft and target heads and
stores immutable references. Reading that preview returns the original
before/proposed versions even if current writing later changes. It does not
renew the preview's freshness. The author separately applies the identified
preview; editing a draft or changing a source makes the old approval stale.

One to three nonchapter targets are validated and adopted within one SQLite
transaction through the material writer shared with Workshop. Working bodies,
before/after revisions, dispositions, author decision, operation receipt, and
one source-epoch advance commit together. A failure on a later target rolls
back every earlier target. Targets governed by Workshop protection retain
those checks. Chapter writes cannot enter this endpoint.

The v3 grouped recipe permits at most three related nonchapter drafts from the
same response. Group effects come only from that response's retained,
validated materialization; the renderer cannot invent or edit an effects
manifest. Endpoint references are exact frozen ordinary source handles or
exact keys for drafts in that response. Preparation resolves them to ordinary
document IDs and heads, records relationship dependencies and protected
content, and retains complete target before/after bodies and metadata. A
successful grouped adoption commits all selected documents and revisions,
supported relationships, the scoped decision, one source-epoch advance, and
the command receipt in the same transaction. The receipt is the authority for
replay and later recap.

Only relationships are currently supported for grouped adoption. Nonempty
impacts, supersessions, or placements are refused during preparation and
cannot trigger organization, moves, renames, deletions, or other automatic
side effects. Their presence in retained provider output remains inspectable
materialization evidence, but it does not create an adoptable preview. This
explicit refusal is preferable to silently dropping an effect or applying a
partial group. Adding another effect type requires its own complete preview,
validation, and atomic commit contract.

Lost acknowledgments retain the same operation identity and preview. Receipt
replay returns the original accepted result. Recovery must read current
Working heads before displaying documents so replay cannot replace later prose
with an old receipt body. If adoption is not confirmed after the active editor
was detached, the workspace restores the exact flushed body/head and uses the
existing fenced `DocumentSession.reconcile()` query. It propagates the rotated
lease and exposes a competing durable commit as an editor conflict. A plain
document read does not settle an unknown mutation. Project/session checks
prevent late results from reactivating another project's editor. Draft
reconnection preserves local text and pending saves while access changes are
propagated.

The saved-document recap is a read-only projection of durable document-save
events and adoption receipts. It does not call an LLM, create a new transcript
message, or generate prose. It links the saved revision and its source event
so returning to the conversation cannot fabricate a summary or change story
authority.

When a preview is stale, **Apply** is disabled while the immutable preview is
retained. **Compare sources/current heads** loads the saved target documents
and reports broad source/policy epoch changes when supplied. A snapshot race
refuses the comparison rather than silently mixing heads; the author must compare
again. **Prepare against current versions** is a separate explicit action and
must pass fresh core validation. A draft marked stale cannot be rebased from
the comparison; it must be refreshed with the assistant. A new target with no
captured before body remains valid when its current saved target is also absent.

Review displays each target in this order: **What changed**, the original
author request and working assumptions, affected documents, the complete
before/after bodies, and the deterministic detailed diff. The original
producing message and its source material remain recoverable by paging the
authoritative conversation history; a later page is not treated as a new
request or a new generation. This keeps draft provenance inspectable even when
the current transcript page no longer contains the origin.

The UI keeps frozen source identity separate from current source identity.
Request context displays the target and captured scope from the immutable
conversation item; attached-context chips display the exact document head and
body hash that will be frozen for a new request. Historical conversation views
render the original source and draft revisions retained with that request.
When a preview is stale, the retained preview remains unchanged while a
separate comparison reads current saved heads. A current head can authorize a
new explicit preparation only after the normal validators pass; it never
silently replaces the frozen evidence in the old preview.

## Chapters, recovery, and rollout

The shared conversation links ordinary chapter discussions to their actual
target and exact selected scope. Selected edits and continuation retain the
existing proposal/preflight/Apply path. Restricted writing receives no
automatic project transcript or assistant draft. Adapting a project message
into a writing brief requires explicit approval and exact message, target,
scope, text, project, and namespace provenance. A restricted chapter response
cannot pose as an AuthorRoom project-message origin.

Chapter feedback can begin without a selection. The chapter discussion
response may contain one optional contiguous `rangeProposal`. Rust treats it as
a read-only hint and validates its exact frozen target head, first and last
editor block IDs, contiguous `Blocks` range, and exact quote. A malformed or
stale hint leaves the readable feedback available but makes the range
unavailable. The Writer shows the validated range for review; **Use this
passage for an edit** re-reads and revalidates the same run, preserves newer
typing, and rejects a changed chapter rather than replacing it. Confirmation
then stages a `proposeEdits` task with that block scope. It does not send a
provider request, edit the chapter, or apply a proposal. The author must still
describe the change and explicitly **Send** the staged task through the normal
chapter validation and Apply flow.

An optional `chapterHandoff` in a project-chat response is preparation only. It
offers an explicit existing ordinary chapter target or a new blank chapter and
an editable proposed title. Preparing it stages the chapter task and its
optional safe brief; approving that brief is separate from **Send**. The author
must explicitly Send through the existing chapter proposal/continuation path,
with the existing target and selected-scope checks. The handoff does not create
canon, dispatch a second request, or grant write authority by itself.

Attaching a source from the Writer flushes the editor first, captures the exact
saved head (replacing the same document's older attached head when necessary),
and returns to the broad project conversation by clearing any staged chapter
task. The staged chapter surface also has an explicit **Return to project
conversation** action; it clears that task without silently sending or applying
anything.

Safe-brief approval is asynchronous and tied to the exact staged chapter
target, selected scope, text, and provenance. Any target or scope change
invalidates pending approval, including changing away and back to the previous
chapter or switching projects. The author must approve the resulting brief
again before **Send**.

The chat shell's project-scoped resizer stores only a local view preference. It
starts at 56%, clamps the conversation panel to 30–70%, and supports Arrow
Left/Right, Home, and End on its keyboard-focusable vertical separator. The
parent's single-surface mobile tabs remain authoritative while both surfaces
stay mounted. Native WebView zoom hotkeys remain enabled, including the
author's normal zoom controls; the zoom and accessibility gates are still
separate qualification work.

On the small-screen surface, the Documents view and staged chapter handoff
have explicit, reliable exits from review. Returning to the conversation
preserves the retained review state without silently applying it and explicitly
focuses the composer; switching to another surface leaves focus on the clicked
navigation control. Opening a chapter preserves the retained review state
without silently applying it.

Backup and duplication retain private history and isolated drafts. Recovery
creates a new project and operation namespace; historical rows do not gain
write or context authority in the recovered project. Existing immutable
packet bytes and historical provider settings are not rewritten.

Chat is an opt-in workspace trial. Existing Develop and Write navigation stay
available. Automated core, renderer, and native checks are separate evidence;
human formative review, physical input/accessibility, live provider behavior,
and installed packaging are separate qualification gates. Making chat the
default requires the specified author evaluation, not only passing mocks.

The current implementation checkpoint includes six focused grouped-effects
checks, including post-write SQL rollback and local retry, alongside the
701-test frontend suite. These are implementation evidence only; native/live,
human, accessibility, and installed-package gates remain separate and are
tracked in the implementation ledger.

Current implementation anchors for these contracts are
`crates/core/src/projects/project_chat_output.rs`,
`crates/core/src/projects/project_chat_context.rs`,
`apps/desktop/src/chat/ChapterHandoff.tsx`,
`apps/desktop/src/chat/ProjectConversation.tsx`,
`apps/desktop/src/chat/conversationStore.ts`,
`apps/desktop/src/chat/ChatSplitPane.tsx`,
`apps/desktop/src/chat/ChapterRangeReview.tsx`,
`apps/desktop/src/chat/confirmChapterRange.ts`,
`apps/desktop/src/shell/Workspace.tsx`,
`apps/desktop/src/shell/RecentProjectPicker.tsx`,
`apps/desktop/src/ipc/projectActivity.ts`,
`apps/desktop/src/assistant/SourceVersionComparison.tsx`, and
`apps/desktop/src-tauri/tauri.conf.json`.
