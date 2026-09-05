# WebnovelStudio V3

WebnovelStudio V3 is the native desktop rewrite of WebnovelStudio for English novel authoring, UI, and export. Optional translated-webnovel, wuxia, and xianxia register or terminology may shape style work later; they are not required genres or language modes. The current checkout is `codex/v3-persistence`: the default native UI now provides a persistent Library/Workspace, while the explicit W0 sample editor remains a session-only trial. W1 structural scope validation and shared JS/Rust fixtures are implemented; W2 session/core receipt and reconciliation work is implemented; W3 registry/transfer work remains active. Story Context C0 contracts and adversarial tests are complete, C1 working-basis snapshot/retrieval is implemented, C2 pure packet compilation is in progress, and C3–C6 remain planned.

The default Library/Workspace supports blank projects, optional chapter/character/world/theme/hook/scene/note documents, autosave through Rust-owned SQLite, flush-before-switch, project/document rename, duplicate, archive/unarchive, and native folder, backup, recovery, and draft-TXT dialogs. W0's sample text and feedback remain session-only; it is not an author-data path and has no provider or durable Apply. W3 schema-3 migration takes a durable pre-upgrade backup, recovers schema-1/2 databases, and carries exact-head caret/last-document state, metadata compare-and-swap, and `context_source_epoch`; copied historical snapshots cannot authorize operations in a new project. C1 keeps original source, uses source-only alias matches, and keeps aliases private to AuthorRoom until safe grants; reviewed/history/character policies are unavailable until authority work, and AuthorRoom Revise/Continue is blocked. The remaining English native author trial, minimum-window behavior, external Word paste, screen-reader use, and broader qualification remain open. The [W0 qualification record](docs/W0_QUALIFICATION.md) is historical; see the [implementation status](docs/IMPLEMENTATION_STATUS.md) for current gates.

The private GitHub repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3), with `main` as its default branch. The current `main` tip is `d0eebfd780e435c068ef1017cac580786360d36b`; active implementation is on `codex/v3-persistence`. V2 remains unchanged in `D:\WebnovelStudio_V2`. The [product requirements](PRODUCT.md), [implementation status](docs/IMPLEMENTATION_STATUS.md), [workspace plan](docs/V3_WORKSPACE_PLAN.md), [first-slice plan](docs/V3_FIRST_SLICE_PLAN.md), and [editor contract](docs/ADR_0001_EDITOR_CONTRACT.md) define the current boundaries.

The adopted [Story Context Engine design](docs/V3_STORY_CONTEXT_SYSTEM.md) and [context implementation plan](docs/V3_STORY_CONTEXT_FIRST_SLICE.md) add retained evidence, frozen request sources, scoped guidance, inspectable packets, and bounded lookup. C0 contracts and C1 working-basis snapshot/retrieval are implemented in the core; C2 packet compilation is in progress, while context UI and provider integration remain planned. Current local and GitHub validation evidence is recorded in [implementation status](docs/IMPLEMENTATION_STATUS.md).

The latest local verification passed the wrapper's rustfmt, workspace Clippy with `-D warnings`, 90 active Rust tests plus one ignored child test, the TypeScript/Vite build, and 79 frontend tests; a full desktop Vitest rerun passed 80/80. The prior rebuilt real Tauri/WebView2 smoke baseline passed 14/14 checks, including two-project save/switch/reload and copy isolation, process kill/restart, project/document rename, exact caret/last-document restore, and archive/unarchive. A later native follow-up changed duplicate-edit polling to wait for an actual ProseMirror transaction; it has not been rerun locally while the debug executable remains open. CI run 33973213684 for `73db1bc` passed the Windows native, Windows, and Ubuntu contract jobs. Native backup/export dialog use and the broader A, N, and W3 qualification remain open.

## Run the spike

From `D:\WebnovelStudio_V3`, use the root wrapper:

```powershell
.\scripts\desktop.ps1 -Command dev
.\scripts\desktop.ps1 -Command spike
.\scripts\desktop.ps1 -Command build
.\scripts\desktop.ps1 -Command check
.\scripts\desktop.ps1 -Command native
.\scripts\desktop.ps1 -Command test
```

The wrapper selects the pinned Node `24.20.0` through `npm exec` and adds the user Cargo bin directory to its child environment. First use can download Node and install the locked frontend dependencies. Use `-Command setup` to refresh those dependencies. Close the trial window before rebuilding its executable on Windows. `native` requires a successful `spike` build and drives its real WebView2 window through a local debugging endpoint.

The pinned development surface is Rust `1.98.1` MSVC (`x86_64-pc-windows-msvc`), Node `24.20.0`, npm `11.12.1`, Tauri `2.11.5` with CLI `2.11.4` and build `2.6.3`, React `19.2.8`, Tiptap `3.31.3`, and the ProseMirror packages locked in `apps/desktop/package.json` and `package-lock.json`. The Tauri runtime observed for the spike is WebView2 `152.0.4191.62` on Windows 11 Pro build `26200`. `rusqlite` `0.40.2` is bundled into the Rust core for file-backed project persistence and backup/recovery; this does not imply that the W0 sample editor itself is durable.

The trial surface is intentionally small: type and format sample prose, insert scene breaks, undo/redo, select a passage, keep feedback, preview or reject a local replacement, apply it in the current editor session, and ask Rust to validate the canonical snapshot. Closing the window clears the manuscript and feedback. Use sample text only.
