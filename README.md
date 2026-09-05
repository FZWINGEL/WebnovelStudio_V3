# WebnovelStudio V3

A native desktop writing application for English webnovels, built with Rust, Tauri, React, and Tiptap. Create several projects, develop story material in any order, and keep the manuscript at the center of the workspace. Wuxia, xianxia, cultivation, and translated-webnovel register are optional English writing styles.

**In development.** The persistent workspace runs locally, and discussion currently uses a clearly labelled deterministic test model. The W5 proposal/review/Apply slice is implemented for development and covered by the pushed [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728); installed-release qualification and production live AI remain unfinished. The separate native Codex discovery and synthetic experiments are recorded in [Codex qualification](docs/CODEX_QUALIFICATION.md). See [implementation status](docs/IMPLEMENTATION_STATUS.md) for exact test evidence and remaining work.

## Available in the development build

- A project library with create, open, rename, duplicate, archive, and resume flows.
- Chapter, character, world, theme, hook, scene, and note documents, with no required creation order.
- Rich-text editing, local SQLite autosave, retained document positions, and flush-before-switch behavior.
- Manual backup, independent recovered projects, and explicit draft export.
- Persistent document discussion, selected-passage feedback, unsent drafts, Stop, and explicit retry using the local test model. Unchanged retries keep their original one-use guidance, and the retry choice survives restart.
- Exact recent completed exchanges in follow-up discussion, with visible omissions when context is limited.
- **Keep as guidance** from a chat message, or directly add a direction. Edit, save, and remove instructions for the next request, this document, or this project.
- **Story context** inspection that distinguishes saved sources available to a request from the exact material supplied for its response.
- **This W5 checkpoint:** suggest edits for a selected chapter passage with deterministic review cards, editable previews, Apply, and Reject; development contracts are covered by [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728), while broader installed-release/native qualification remains pending.
- **Current W6 development slice:** compare saved document versions and explicitly restore a selected version through History; see the [document history contract](docs/ADR_0005_DOCUMENT_HISTORY.md). Final W6 qualification remains pending.
- **Current W7 development slice:** inspect exact Markdown or plain-text output before choosing a destination; retain the frozen revision and export metadata. Existing files are preserved. See the [draft export contract](docs/ADR_0006_DRAFT_EXPORT.md).

Guidance is an explicit author choice and never changes manuscript text or establishes canon. Requests retain the exact instruction versions and prior exchanges they used. Restricted chapter-passage edit requests use a reader frontier and exclude author-room private material, future material, current guidance, and recent chat. Persistent chapter/project source pins, generated memory digests, and bounded live-provider lookups remain planned parts of the [Story Context Engine](docs/V3_STORY_CONTEXT_SYSTEM.md). The V2 Codex adapter/model-picker remains an authorized reference; [Codex qualification](docs/CODEX_QUALIFICATION.md) records native discovery and bounded synthetic experiments, while production discussions remain mock-only and live-provider support stays unqualified.

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

Toolchain and dependency versions are pinned in the root manifests and desktop lockfile. Author databases, backups, credentials, and generated native results must stay out of Git.
