# WebnovelStudio V3

A native desktop writing application for English webnovels, built with Rust, Tauri, React, and Tiptap. Create several projects, develop story material in any order, and keep the manuscript at the center of the workspace. Wuxia, xianxia, cultivation, and translated-webnovel register are optional English writing styles.

## Current chat-first development surface

The project conversation is implemented as an **opt-in** native development
surface. One conversation can handle project questions and chapter feedback;
chapter requests retain their exact chapter and selected scope. Assistant
responses become isolated task drafts that can be edited, reviewed, and adopted
explicitly, including grouped adoption for up to three nonchapter documents.
Scoped question/assumption decisions and read-only conversation history are
available. The default workspace remains unchanged while native and formative
qualification continues. Stale drafts require an explicit refresh before a
new proposal, and an unchanged old adoption preview stays blocked after its
source changes. The draft review inventory is complete and older conversation
timeline entries load through paging. See the [chat-first implementation
status](docs/V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md) and [ADR
0034](docs/ADR_0034_PROJECT_CONVERSATION.md).

The conversation can attach an exact source from the Writer, stage an explicit
chapter handoff, and review the original request, assumptions, affected
documents, complete before/after bodies, and deterministic diff before Apply.
The project-scoped conversation/document resizer is keyboard accessible, while
the small-screen surface uses one visible panel at a time.

The current project database reader floor is **schema 40**. Schema 39 adds
document roles and schema 40 adds project-conversation and assistant-draft
records; existing document and history identities remain intact.

The authoritative [chat-first implementation status](docs/V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md)
records the current Rust, frontend, native, live-provider, and build-identity
evidence. Those checks are development evidence only. The chat surface remains
opt-in while human formative evaluation, installed-package behavior, screen
reader behavior, and broader provider/narrative qualification continue. A local
native 200% zoom geometry check is verified; it does not qualify physical
screen-reader use or an author trial. English-only authoring means IME
qualification is not a product requirement.

**3.0.0 private Windows development candidate — unreleased.** Direct the AI, review its proposals, and apply the changes you choose. The workspace supports persistent discussion, selected-text feedback, offline writing, and a model picker with reasoning and service-tier controls. Codex uses your installed CLI without pinning its version; Claude and configurable OpenAI-compatible endpoints are also available as development integrations.

See the [release preparation](docs/RELEASE_3_0_0.md) and [changelog](CHANGELOG.md) for candidate scope. The [implementation status](docs/IMPLEMENTATION_STATUS.md) records exact test runs and remaining qualification. Synthetic HTTP checks pass; they do not establish behavior for a particular live service, and live Claude qualification remains open.

The Story Context Engine retains original evidence, freezes each request’s permitted sources, and records the exact input sent to the assistant. Generate source-linked chapter memory explicitly, inspect what a discussion received, and keep generated summaries separate from author-reviewed material. New native Codex maintenance uses the fixed GPT-6 Astra/low/priority profile even when the author selects another writing model; configured HTTP Story Memory uses Astra/low without a service tier.

In Story Workshop, keep a consequence, reject its assumption, or prepare a
contrast before explicitly exploring it. Relationship explorations include both
participants' preferences. Archived choices retain Keep fixed until you remove
it, and questions set aside stay that way until reopened. Current implementation
and qualification are recorded in the [Workshop ledger](docs/V3_STORY_WORKSHOP_IMPLEMENTATION.md).

Candidate cards also offer **Explore another angle → Give alternatives** for
refining that candidate along the current comparison dimension. The request
leaves your existing selection, working version, and story choices unchanged.

Keep unresolved questions, intended payoffs, and possible arcs as separate
editable story possibilities. Prepare one for exploration when you want AI
suggestions; keeping or reopening it does not establish a story event. Themes
offer separate reader-experience and content-intensity preferences, with your
editable wording saved only after confirmation. On smaller windows the context
drawer keeps keyboard focus inside and returns it when closed. See the
[Workshop completion record](docs/V3_STORY_WORKSHOP_COMPLETION.md) for current
checks and remaining native, provider, and author-evaluation gates.

Codex is the primary provider focus. Claude and configurable OpenAI-compatible
endpoints remain available as development integrations; further adapter ports
are deferred.

An optional persistent Codex transport is now available in **Settings → Request
transport → Codex app-server**. Select it, then use **Check Codex connection**.
It keeps the local process warm and starts a fresh conversation for each request.
Your model, reasoning effort, and response-speed choices remain independent.
Exec remains the default, and bounded story lookup still requires Exec. See
[app-server qualification](docs/APP_SERVER_QUALIFICATION.md) for the tested
behavior and remaining development gates.

Chapter continuation offers Working/Reviewed story choices, an editable preview, and explicit Apply/Reject. Exports offer working drafts or exact author-reviewed snapshots. Reuse an object or promise across chapters and inspect its passage-backed history. Structured suggestions provide explicit paragraph or whole-chapter scopes with a rich editable preview and protected surrounding blocks. A C6 development slice now adds an opt-in bounded story lookup route for Working, AuthorRoom, and Discuss conversations; its protocol, packet compiler, persistence, and focused UI/native checks are implemented and tested, while broader qualification remains open. See [implementation status](docs/IMPLEMENTATION_STATUS.md) for the evidence and qualification boundaries. Richer memory, wider edit scopes, broader provider support, author trials, and full release qualification remain open.

## Available in the development build

- A project library with create, open, rename, duplicate, archive, and resume flows.
- Chapter, character, world, theme, hook, scene, and note documents, with no required creation order.
- Rich-text editing, local SQLite autosave, retained document positions, and flush-before-switch behavior.
- **Story Workshop (development slice):** Develop/Write workbench with six lenses—Overview, World, People, Themes & tone, Story possibilities, and Notebook—for manual exploration and explicit generation. Requests produce typed candidates and reviewable alternatives; only an explicit preview and adoption changes nonchapter documents. Atomic add/replace adoption supports existing and new relationship endpoints, exact source-head checks, and reviewable impact categories. World/People exploration prepares a named directional relationship with both endpoint heads, while Try a moment requires two or three noncanon treatments. Optional Unicode names, aliases, and transliterations are saved in the existing `document_aliases` metadata (no schema change) on character/world documents through the World/People saved-material picker and Writer’s **Names & aliases** surface. Saving uses a source-epoch CAS with read-only reconciliation for uncertain outcomes, protects dirty navigation/close, and leaves title/body unchanged; restricted-context packets exclude aliases. Author-room decisions and protected details remain separate from writing eligibility, character knowledge, and manuscript evidence. Workshop persistence is retained in the current schema 40 reader floor; its migration preserves older Workshop state, context, packet bytes, and hashes. See the [Story Workshop implementation ledger](docs/V3_STORY_WORKSHOP_IMPLEMENTATION.md).
- **Workshop qualification evidence:** the 8 September isolated Windows desktop run passed all 31 native Tauri/WebView2 Workshop checks, including names/aliases, using the deterministic mock provider. A separate four-check native Settings run verified transport/model-trait persistence and restart without generation. These checks do not establish full native live-provider, installed-release, or author-study acceptance. Current reports and earlier failed attempts are recorded in [app-server qualification](docs/APP_SERVER_QUALIFICATION.md); earlier hosted checkpoints remain in the [Workshop ledger](docs/V3_STORY_WORKSHOP_IMPLEMENTATION.md).
- **Bounded live Workshop checks:** the existing Exec check and one new app-server Luna/xhigh/priority check each returned three valid directions with saved packet and delivery evidence, without changing manual work or creating chapters. The app-server trial also confirmed shared-server shutdown. These are synthetic provider/core checks; broader live and narrative-quality evidence remains open. See [Codex qualification](docs/CODEX_QUALIFICATION.md) and [app-server qualification](docs/APP_SERVER_QUALIFICATION.md).
- **Current private installer:** [package run 34150884037](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150884037) passed on exact product source `fc3468829e037588458a05204c0ef92ddbea9cc2`, with installer SHA-256 `4c506a51da20a189a31d7d738786401024457307c637bb188acdeea49443aec6`. Installed writing, reopen, normal close, and same-version uninstall/reinstall retention passed. Complete Workshop native acceptance, upgrades, live-provider behavior, and author evaluation remain open. Exact evidence is in [Windows package qualification](docs/WINDOWS_PACKAGE_QUALIFICATION.md).
- Manual backup, independent recovered projects, and explicit draft export.
- Persistent document discussion, selected-passage feedback, unsent drafts, Stop, and explicit retry. Queued Stop seals immediately; running Stop settles with retained partial output. Failed local response saves have an explicit local retry that does not send another model request. Unchanged retries keep their original one-use guidance, and the retry choice survives restart.
- Exact recent completed exchanges in follow-up discussion, with visible omissions when context is limited.
- **Keep as guidance** from a chat message, or directly add a direction. Edit, save, and remove instructions for the next request, this document, or this project.
- **Story sources** saved for discussions about this document or the whole project, with explicit confirmation and removal.
- **Story context** inspection that distinguishes saved sources available to a request from the exact material supplied for its response.
- **Writing briefs for selected edits:** in `Suggest edits`, an author can adapt an author-room message or reply into editable directions, explicitly approve the text, and send only that exact brief with the restricted selected-chapter-passage request. Ordinary edit requests remain available without a brief.
- **Passage suggestions:** review alternatives for a selected single-line chapter passage, edit a preview, then Apply or Reject. Applying preserves surrounding text and leaves other suggestions pending and stale.
- **Structured suggestions:** explicitly choose **Selected paragraphs** or **Whole chapter**, edit a rich preview containing headings, marks, scene breaks, and hard breaks, then Apply or Reject while preserving protected surrounding blocks. See the [structured suggestions contract](docs/ADR_0020_STRUCTURED_SUGGESTIONS.md).
- **Author review (F2-A development slice):** stage an immutable exact saved chapter, inspect exact earlier selected revision previews, and explicitly **Mark this version reviewed**. Review is optional for writing; a saved stage can be resumed after restart, changed earlier selections are flagged, and recovered or duplicated copies clear active review heads. See the [author review contract](docs/ADR_0012_AUTHOR_REVIEW.md).
- **Reviewed context (F2-B core slice):** freeze the exact earlier reviewed prefix and current working target through the Rust/IPC boundary, with immutable reader-position pins and historical namespace fencing. The schema-19 story-continuation slice consumes this boundary for its explicit Reviewed basis; broader provider and release qualification remain open. The schema-20 reviewed export is a qualified historical development slice and does not establish canon or publication. See the [reviewed context contract](docs/ADR_0013_REVIEWED_CONTEXT.md).
- **Passage-backed reviewed story details (schema-21 development slice):** while reviewing a chapter, select a passage, record what object or holder it describes, choose whether the detail is visible to the reader, and inspect the reviewed evidence later. The saved details remain observations supported by passages; they do not claim a complete account of the story. See the [reviewed story evidence contract](docs/ADR_0018_REVIEWED_STORY_EVIDENCE.md).
- **Character knowledge:** record what a character knows, believes, suspects, rejects or is explicitly unaware of during optional chapter review. Reuse character/topic identities, retain exact passages, and inspect recorded history. Restricted continuation receives only reader-visible observations from earlier eligible chapters; absent evidence never becomes a claim of unawareness. See [ADR 0031](docs/ADR_0031_CHARACTER_KNOWLEDGE.md).
- **Object history (C5-A development slice):** reuse an object across chapters, inspect its recorded holders and exact passages, and open the saved source. Uncertain timing and unknown holders stay visible. See the [evidence history contract](docs/ADR_0019_EVIDENCE_HISTORY.md).
- **Promise history (schema-23 C5 development slice):** while reviewing chapters, record setup, payoff, cancellation, or unclear observations against an explicitly chosen promise, choose reader visibility, reuse that identity across chapters, and inspect its exact recorded history. Local native development checks cover this boundary, and one bounded Luna discussion check delivered the reviewed promise evidence while preserving prose. AuthorRoom packets can show frozen source names and promise-set source names; restricted packets omit those names while applying the same record and source-policy boundaries. Missing payoffs do not prove that a promise is unresolved. See the [promise history contract](docs/ADR_0021_PROMISE_HISTORY.md).
- **Story continuation (committed development slice):** choose Working draft or Reviewed story in the chapter assistant, receive one typed append-only candidate, edit its paragraphs, and Apply or Reject it without rewriting existing blocks. Schema 19 persists the continuation kind and prepared paragraphs; retries retain the selected basis, operation identity, generated IDs, and exact body after an uncertain acknowledgment. Earlier local and hosted continuation evidence is retained in [implementation status](docs/IMPLEMENTATION_STATUS.md); broader live-provider and full native/release qualification remain pending. See the [story continuation contract](docs/ADR_0016_STORY_CONTINUATION.md).
- **Accepted narrative summaries:** add an optional summary in Story review, or explicitly copy current generated memory as editable starting text. Save and resume the review before accepting it. Accepted summaries keep their exact chapter and earlier reviewed basis, with separate audience-filtered context coverage; they never replace the manuscript. See [ADR 0030](docs/ADR_0030_ACCEPTED_SUMMARIES.md).
- **Story memory (C4 development slices):** use **Refresh story memory** explicitly from a chapter panel to generate a bounded `navigation-digest.v1` view for the full current chapter revision. Rust validates exact UTF-16 evidence quotations and source identity; jobs, terminal results, and views are separate records, stale or revoked output is fenced, and local save/install retry never redispatches the model. Current author project data is schema 40; library model preferences and endpoint profiles use library schema 4. Schema 28 preserves historical Codex packet bytes while adding the frozen source-title reader boundary, schema 29 adds optional HTTP delivery receipts for memory results, and schema 30 adds nullable Claude reported-model evidence. Story Memory provider choice is independent from the writing picker: fixed Codex Astra/low/priority, explicit offline mock, or configured HTTP; missing model, key, or configuration remains unavailable with no fallback. The HTTP binding captures its route/configuration and private credential at acceptance, retains at most 2 MiB input and 64 KiB output, and never replays a POST during local recovery. Claude author requests do not provide memory or lookup; Astra maintenance remains independent. The view supports source inspection as an unreviewed navigation aid. Working author-room discussions can reuse current non-target chapter views when full prose will not fit, with exact frozen coverage and evidence; restricted writing and reviewed continuation exclude them. See [ADR 0025](docs/ADR_0025_API_STORY_MEMORY.md), the [navigation context contract](docs/ADR_0015_NAVIGATION_CONTEXT.md), and the [chapter memory contract](docs/ADR_0014_CHAPTER_MEMORY.md).
- **Bounded story lookup (C6 development slice):** Working author-room discussions can opt in to local searches, exact passage reads, and reviewed character knowledge, promise and possession history. Rust uses one frozen story snapshot and at most three model invocations. The assistant can find explicit identities and page whole observations; missing records remain uncertain. Each call retains its exact request, results and delivery evidence for inspection. The choice is off by default. Restricted-writing lookup, broader provider and narrative-quality qualification remain open. See [ADR 0022](docs/ADR_0022_BOUNDED_STORY_LOOKUPS.md), [reviewed-memory lookups](docs/ADR_0032_STORY_MEMORY_LOOKUPS.md), and [current evidence](docs/IMPLEMENTATION_STATUS.md).
- **Current W6 development slice:** compare saved document versions and explicitly restore a selected version through History; see the [document history contract](docs/ADR_0005_DOCUMENT_HISTORY.md). Final W6 qualification remains pending.
- **Current W7 development slice:** inspect exact Markdown or plain-text output before choosing a destination; retain the frozen revision and export metadata. Existing files are preserved. Schema-20 reviewed export now covers exact author-reviewed chapter snapshots in the local native diagnostic; the local wrapper also passes, while strict CI 34010306332 passes both contract jobs and all 38 native checks. See the [draft export contract](docs/ADR_0006_DRAFT_EXPORT.md) and [reviewed export ADR](docs/ADR_0017_REVIEWED_EXPORT.md).
- **Provider and model direction:** the V2-style provider rail, model search, favorites, keyboard selection, and separate traits are implemented for explicit author-facing model choice. Settings can define OpenAI-compatible endpoint profiles with endpoint URL, model catalog entries, capability metadata, and credentials held by the native credential store; the native HTTP worker is the development transport. Story Memory uses its independent provider preference and fixed Astra/low settings, with priority only on the Codex route. Codex discovery runs from an explicit Settings check or the narrowly guarded read-only startup check, using a bounded paginated app-server probe; the sanitized catalog is display-only until the checked connection authorizes the exact selection. Claude author development now exposes static Fable 5, Opus 5, and Sonnet 5 rows with low/medium/high/xhigh/max effort choices; an unchecked native connection keeps sending blocked, and Claude has no memory or lookup route. See [Claude qualification](docs/CLAUDE_QUALIFICATION.md) and [ADR 0026](docs/ADR_0026_CLAUDE_AUTHOR.md). HTTP context lookup is not supported yet; hosted HTTP qualification, broader live coverage, dynamic-provider/live qualification, and installed-release qualification remain open. Additional V2 CLI adapter ports are deferred. See [ADR 0023](docs/ADR_0023_OPENAI_COMPATIBLE.md), [ADR 0024](docs/ADR_0024_DYNAMIC_CODEX_MODELS.md), and [ADR 0025](docs/ADR_0025_API_STORY_MEMORY.md).
- **Codex connection:** an explicit Settings check or narrowly guarded read-only startup check, selected model/traits, isolated Windows process execution, bounded output, and durable outcome/usage records. Startup adoption of Luna/xhigh/priority applies only to an untouched writing choice; explicit choices remain unchanged. Author requests use `codex-stdin.author.v1` with concrete resolved traits and a sanitized descriptor fingerprint; missing models or traits remain selected and unavailable for explicit repair. Unreported usage and effective settings remain unknown. The app does not silently substitute another provider or model.
- **AI writing workspace (in development):** user-directed **Draft**, **Continue**, and **Develop** actions across per-project Chapters, Worldbuilding, Characters, Plot & themes, and Notes tabs. Chapters share ordered navigation; new generation requires explicit **Send**, and returned candidates require review before **Apply**. Navigation and startup checks never send an automatic LLM request. See [ADR 0027](docs/ADR_0027_AI_WRITING_WORKSPACE.md); the schema-31 reader fence, local contracts, native regression, and one real Codex chapter draft are verified. Broader release and author qualification remain open.
- **Persistence floor:** the current project database reader floor is schema 40, and the library/preferences schema is 4. Schema 38 adds separate app-server delivery and immutable dispatch records, schema 39 adds document roles, and schema 40 adds project-conversation and assistant-draft records. Historical packets, hashes, Workshop state, provider bindings, and receipts remain unchanged. Transport preferences use the existing library settings storage.
- **V2 import:** preview a stable schema-8 source, choose a saved draft or an empty chapter where working text is missing, and import into an independent V3 project. Original approvals remain history. Source access and supported-format limitations are documented in [V2 import](docs/V2_IMPORT_PREVIEW.md).

Guidance is an explicit author choice and never changes manuscript text or establishes canon. Requests retain the exact instruction versions and prior exchanges they used. Restricted chapter-passage edit requests use a reader frontier and exclude author-room private material, future material, current guidance, saved discussion sources, and recent chat. An optional brief is sent as exact `approvedWritingBrief` text only after explicit approval; its origin ID, private chat, pins, and guidance stay local, and editing the text or scope clears approval. C4-A chapter navigation memory and the C4-B/C4-C Working-discussion reuse slices are implemented development surfaces; generated views remain non-authority. The C6 lookup route is an opt-in development implementation with strict application-controlled search/read and reviewed-memory messages and fresh invocation records; focused/local native evidence exists, while hosted, live-provider, and broader state and restricted-writing qualification remain open. The schema-20 reviewed export and schema-21 reviewed details are CI-qualified development slices; neither promotes prose to canon or publication.

The separately opened **sample editor trial** demonstrates session-only replacement preview, local Apply, and undo. Its sample prose disappears on close. The default Library/Workspace provides durable proposal review and single-author Apply/Reject for the selected-passage W5 development slice; the sample trial remains session-only.

## Connect an OpenAI-compatible API

1. Open **Settings → API connections → Add API connection**. Enter a connection
   name, the service's **Base URL** (such as `http://localhost:1234/v1`), and its
   API key if required. Use the base URL, not the full `/chat/completions` URL.
2. Enter **Model IDs**, one per line, then choose **Save connection**. You can
   also use **Find models** when the service supports model discovery.
3. Choose that connection's model in the top-bar model picker. Writing and
   revision requests now use this endpoint; no Codex installation is required.
4. To use the endpoint for summaries too, ensure it supports `gpt-6-astra`
   with `low` reasoning and lists that exact model ID. Under **Story memory
   and summaries**, select the connection as **Maintenance provider**.

Enable **Request JSON mode for suggestions** only when the service supports it.
The app uses the Chat Completions interface and streaming responses. A service's
compatibility label does not establish support for every feature; unsupported
settings fail visibly without switching to another model.

## Run locally on Windows

From `D:\WebnovelStudio_V3`:

```powershell
.\scripts\desktop.ps1 -Command dev
```

The wrapper selects the pinned Node version, adds the user Cargo bin directory to its child environment, and installs locked frontend dependencies when needed. Rust/MSVC and the Windows build prerequisites are required. Use `-Command setup` to refresh frontend dependencies.

Other development commands:

```powershell
.\scripts\desktop.ps1 -Command check   # Rust checks, TypeScript/build, frontend tests
.\scripts\desktop.ps1 -Command spike   # Build the native development executable
.\scripts\desktop.ps1 -Command native  # Exercise that executable in real WebView2
.\scripts\desktop.ps1 -Command build   # Build the release profile
.\scripts\desktop.ps1 -Command package # Build a Windows x64 NSIS installer
```

Close an executable before rebuilding it on Windows. `native` requires a successful `spike` build and uses synthetic projects through a local debugging endpoint. To run the chat-first native smoke against that rebuilt executable:

```powershell
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-chat-smoke.mjs
```

The opt-in live Codex trial is separate and submits exactly two requests; run it only when deliberately authorizing live usage:

```powershell
$previous = $env:WNS_V3_ALLOW_LIVE_CHAT
try {
    $env:WNS_V3_ALLOW_LIVE_CHAT = '1'
    npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-chat-live.mjs
} finally {
    $env:WNS_V3_ALLOW_LIVE_CHAT = $previous
}
```

These checks are development evidence; they do not qualify an installed release, broad live-provider behavior, or literary quality.

See [development checks](docs/TESTING.md) for focused test commands and the grouped Rust test harness. The full check still runs every test.

Release builds use a stable library under `%LOCALAPPDATA%\com.webnovelstudio.v3`; development builds keep their separate checkout-specific data. The installer configuration includes an offline WebView2 installer. Package build and installed-release evidence are tracked in [Windows package qualification](docs/WINDOWS_PACKAGE_QUALIFICATION.md).

## Project and design

V3 lives in its own [private GitHub repository](https://github.com/FZWINGEL/WebnovelStudio_V3). Active implementation is on `codex/v3-persistence`; the default branch is `main`. V2 remains the separate reference at `D:\WebnovelStudio_V2`.

- [Product requirements](PRODUCT.md)
- [3.0.0 release preparation](docs/RELEASE_3_0_0.md) and [changelog](CHANGELOG.md)
- [Current implementation and qualification evidence](docs/IMPLEMENTATION_STATUS.md)
- [Architecture and document index](docs/README.md)
- [Workspace arrangement](docs/V3_WORKSPACE_PLAN.md)
- [Implementation order](docs/V3_FIRST_SLICE_PLAN.md)
- [Story Context first slice](docs/V3_STORY_CONTEXT_FIRST_SLICE.md)
- [Editor contract](docs/ADR_0001_EDITOR_CONTRACT.md) and [author guidance contract](docs/ADR_0002_AUTHOR_GUIDANCE.md)
- [Proposal and Apply contract](docs/ADR_0004_PROPOSAL_APPLY.md)
- [Author-approved writing brief contract](docs/ADR_0008_WRITING_BRIEF.md)

Toolchain and dependency versions are pinned in the root manifests and desktop lockfile. Author databases, backups, credentials, and generated native results must stay out of Git.
