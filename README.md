# WebnovelStudio V3

A native desktop writing application for English webnovels, built with Rust, Tauri, React, and Tiptap. Create several projects, develop story material in any order, and keep the manuscript at the center of the workspace. Wuxia, xianxia, cultivation, and translated-webnovel register are optional English writing styles.

**In development.** The desktop workspace provides durable offline writing, project management, discussion, passage suggestions, and explicit Apply/Reject. A bounded Codex development connection is available for GPT-5.6-Luna with Max reasoning and Fast response speed; it uses the supported installed Codex version and an explicit connection check in Settings. The deterministic local test model remains available offline.

The Story Context Engine retains original evidence, freezes each request's permitted sources, and records the exact input sent to the assistant. You can also review and retain an exact chapter version without running a model. The core can freeze an exact reviewed basis consisting of earlier reviewed chapters plus the current working target over IPC; its continuation UI, live dispatch, and ready export remain open. The C4-A Story Memory panel now provides an explicit, source-linked navigation digest for one full current chapter, using the selected local mock or qualified Luna/Max/Fast profile; generated views remain inspection-only and are not supplied to later model packets. Accepted story facts, broader C4 derived-view integration, typed reviewed records, C5/C6 work, and full release qualification remain open. See [implementation status](docs/IMPLEMENTATION_STATUS.md) for executed checks and [Codex qualification](docs/CODEX_QUALIFICATION.md) for the precise live-provider boundary.

## Available in the development build

- A project library with create, open, rename, duplicate, archive, and resume flows.
- Chapter, character, world, theme, hook, scene, and note documents, with no required creation order.
- Rich-text editing, local SQLite autosave, retained document positions, and flush-before-switch behavior.
- Manual backup, independent recovered projects, and explicit draft export.
- Persistent document discussion, selected-passage feedback, unsent drafts, Stop, and explicit retry. Queued Stop seals immediately; running Stop settles with retained partial output. Failed local response saves have an explicit local retry that does not send another model request. Unchanged retries keep their original one-use guidance, and the retry choice survives restart.
- Exact recent completed exchanges in follow-up discussion, with visible omissions when context is limited.
- **Keep as guidance** from a chat message, or directly add a direction. Edit, save, and remove instructions for the next request, this document, or this project.
- **Story sources** saved for discussions about this document or the whole project, with explicit confirmation and removal.
- **Story context** inspection that distinguishes saved sources available to a request from the exact material supplied for its response.
- **Writing briefs for selected edits:** in `Suggest edits`, an author can adapt an author-room message or reply into editable directions, explicitly approve the text, and send only that exact brief with the restricted selected-chapter-passage request. Ordinary edit requests remain available without a brief.
- **Passage suggestions:** review alternatives for a selected single-line chapter passage, edit a preview, then Apply or Reject. Applying preserves surrounding text and leaves other suggestions pending and stale.
- **Author review (F2-A development slice):** stage an immutable exact saved chapter, inspect exact earlier selected revision previews, and explicitly **Mark this version reviewed**. Review is optional for writing; a saved stage can be resumed after restart, changed earlier selections are flagged, and recovered or duplicated copies clear active review heads. See the [author review contract](docs/ADR_0012_AUTHOR_REVIEW.md).
- **Reviewed context (F2-B core slice):** freeze the exact earlier reviewed prefix and current working target through the Rust/IPC boundary, with immutable reader-position pins and historical namespace fencing. No continuation control, live dispatch, or ready export is exposed yet. See the [reviewed context contract](docs/ADR_0013_REVIEWED_CONTEXT.md).
- **Story memory (C4-A/B development slices):** use **Refresh story memory** explicitly from a chapter panel to generate a bounded `navigation-digest.v1` view for the full current chapter revision. Rust validates exact UTF-16 evidence quotations and source identity; jobs, terminal results, and views are separate records, stale or revoked output is fenced, and local save/install retry never redispatches the model. Author project data is schema 17; library model preferences remain schema 2. The view supports source inspection as an unreviewed navigation aid. Working author-room discussions can reuse current non-target chapter views when full prose will not fit, with exact frozen coverage and evidence; restricted writing and reviewed continuation exclude them. Any story edit currently requires refreshing memory before it can be reused; separating chapter-only freshness is the next increment. See the [navigation context contract](docs/ADR_0015_NAVIGATION_CONTEXT.md). See the [chapter memory contract](docs/ADR_0014_CHAPTER_MEMORY.md).
- **Current W6 development slice:** compare saved document versions and explicitly restore a selected version through History; see the [document history contract](docs/ADR_0005_DOCUMENT_HISTORY.md). Final W6 qualification remains pending.
- **Current W7 development slice:** inspect exact Markdown or plain-text output before choosing a destination; retain the frozen revision and export metadata. Existing files are preserved. See the [draft export contract](docs/ADR_0006_DRAFT_EXPORT.md).
- **Codex connection:** explicit Settings check, exact saved model/traits, isolated Windows process execution, bounded output, and durable outcome/usage records. Unreported usage and effective settings remain unknown. Other model choices stay saved but unavailable; the app never substitutes one.
- **V2 import:** preview a stable schema-8 source, choose a saved draft or an empty chapter where working text is missing, and import into an independent V3 project. Original approvals remain history. Source access and supported-format limitations are documented in [V2 import](docs/V2_IMPORT_PREVIEW.md).

Guidance is an explicit author choice and never changes manuscript text or establishes canon. Requests retain the exact instruction versions and prior exchanges they used. Restricted chapter-passage edit requests use a reader frontier and exclude author-room private material, future material, current guidance, saved discussion sources, and recent chat. An optional brief is sent as exact `approvedWritingBrief` text only after explicit approval; its origin ID, private chat, pins, and guidance stay local, and editing the text or scope clears approval. C4-A chapter navigation memory is implemented as an inspection-only development slice; broader derived-view packet integration and bounded live-provider read loops remain open parts of the [Story Context Engine](docs/V3_STORY_CONTEXT_SYSTEM.md).

The separately opened **sample editor trial** demonstrates session-only replacement preview, local Apply, and undo. Its sample prose disappears on close. The default Library/Workspace provides durable proposal review and single-author Apply/Reject for the selected-passage W5 development slice; the sample trial remains session-only.

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

Close an executable before rebuilding it on Windows. `native` requires a successful `spike` build and uses synthetic projects through a local debugging endpoint. These checks are development evidence; they do not qualify an installed release, a live provider, or literary quality.

Release builds use a stable library under `%LOCALAPPDATA%\com.webnovelstudio.v3`; development builds keep their separate checkout-specific data. The installer configuration includes an offline WebView2 installer. Package build and installed-release evidence are tracked in [Windows package qualification](docs/WINDOWS_PACKAGE_QUALIFICATION.md).

## Project and design

V3 lives in its own [private GitHub repository](https://github.com/FZWINGEL/WebnovelStudio_V3). Active implementation is on `codex/v3-persistence`; the default branch is `main`. V2 remains the separate reference at `D:\WebnovelStudio_V2`.

- [Product requirements](PRODUCT.md)
- [Current implementation and qualification evidence](docs/IMPLEMENTATION_STATUS.md)
- [Architecture and document index](docs/README.md)
- [Workspace arrangement](docs/V3_WORKSPACE_PLAN.md)
- [Implementation order](docs/V3_FIRST_SLICE_PLAN.md)
- [Story Context first slice](docs/V3_STORY_CONTEXT_FIRST_SLICE.md)
- [Editor contract](docs/ADR_0001_EDITOR_CONTRACT.md) and [author guidance contract](docs/ADR_0002_AUTHOR_GUIDANCE.md)
- [Proposal and Apply contract](docs/ADR_0004_PROPOSAL_APPLY.md)
- [Author-approved writing brief contract](docs/ADR_0008_WRITING_BRIEF.md)

Toolchain and dependency versions are pinned in the root manifests and desktop lockfile. Author databases, backups, credentials, and generated native results must stay out of Git.
