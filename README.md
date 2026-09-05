# WebnovelStudio V3

A native desktop writing application for English webnovels, built with Rust, Tauri, React, and Tiptap. Create several projects, develop story material in any order, and keep the manuscript at the center of the workspace. Wuxia, xianxia, cultivation, and translated-webnovel register are optional English writing styles.

**In development.** The persistent workspace runs locally, and discussion currently uses a clearly labelled deterministic test model. Live AI, durable suggestion Apply, and release qualification are unfinished. See [implementation status](docs/IMPLEMENTATION_STATUS.md) for exact test evidence and remaining work.

## Available in the development build

- A project library with create, open, rename, duplicate, archive, and resume flows.
- Chapter, character, world, theme, hook, scene, and note documents, with no required creation order.
- Rich-text editing, local SQLite autosave, retained document positions, and flush-before-switch behavior.
- Manual backup, independent recovered projects, and explicit draft-TXT export.
- Persistent document discussion, selected-passage feedback, unsent drafts, Stop, and explicit retry using the local test model. Unchanged retries keep their original one-use guidance, and the retry choice survives restart.
- Exact recent completed exchanges in follow-up discussion, with visible omissions when context is limited.
- **Keep as guidance** from a chat message, or directly add a direction. Edit, save, and remove instructions for the next request, this document, or this project.
- **Story context** inspection that distinguishes saved sources available to a request from the exact material supplied for its response.

Guidance is an explicit author choice and never changes manuscript text or establishes canon. Requests retain the exact instruction versions and prior exchanges they used. Restricted writing excludes private author-room guidance and conversation. Persistent chapter/project source pins, generated memory digests, and bounded live-provider lookups remain planned parts of the [Story Context Engine](docs/V3_STORY_CONTEXT_SYSTEM.md).

The separately opened **sample editor trial** demonstrates session-only replacement preview, local Apply, and undo. Its sample prose disappears on close. The default Library/Workspace uses persistent projects; it does not yet have durable suggestion Apply.

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
```

Close an executable before rebuilding it on Windows. `native` requires a successful `spike` build and uses synthetic projects through a local debugging endpoint. These checks are development evidence; they do not qualify an installed release, a live provider, or literary quality.

## Project and design

V3 lives in its own [private GitHub repository](https://github.com/FZWINGEL/WebnovelStudio_V3). Active implementation is on `codex/v3-persistence`; the default branch is `main`. V2 remains the separate reference at `D:\WebnovelStudio_V2`.

- [Product requirements](PRODUCT.md)
- [Current implementation and qualification evidence](docs/IMPLEMENTATION_STATUS.md)
- [Architecture and document index](docs/README.md)
- [Workspace arrangement](docs/V3_WORKSPACE_PLAN.md)
- [Implementation order](docs/V3_FIRST_SLICE_PLAN.md)
- [Story Context first slice](docs/V3_STORY_CONTEXT_FIRST_SLICE.md)
- [Editor contract](docs/ADR_0001_EDITOR_CONTRACT.md) and [author guidance contract](docs/ADR_0002_AUTHOR_GUIDANCE.md)

Toolchain and dependency versions are pinned in the root manifests and desktop lockfile. Author databases, backups, credentials, and generated native results must stay out of Git.
