# WebnovelStudio V3

WebnovelStudio V3 is the native desktop rewrite of WebnovelStudio for English novel authoring, UI, and export. Optional translated-webnovel, wuxia, and xianxia register or terminology may shape style work later; they are not required genres or language modes. The current checkout is the W0 native editor spike: a Tauri window with a React/Tiptap manuscript editor, a small feedback panel, and a Rust snapshot validation command over real IPC.

W0 is **in progress**. The window uses sample text held in memory for the session. It has no author storage, project library, provider, SQLite persistence, durable Apply path, receipts, or reconciliation. The remaining native author trial, minimum-window behavior, external Word paste, screen-reader use, and broader qualification remain open. See the [W0 qualification record](docs/W0_QUALIFICATION.md).

The implementation branch is `codex/v3-native-editor-spike`. V2 remains unchanged in `D:\WebnovelStudio_V2`; V3 has no GitHub remote. The [product requirements](PRODUCT.md), [workspace plan](docs/V3_WORKSPACE_PLAN.md), [first-slice plan](docs/V3_FIRST_SLICE_PLAN.md), and [editor contract](docs/ADR_0001_EDITOR_CONTRACT.md) define the next steps.

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

The pinned development surface is Rust `1.98.1` MSVC (`x86_64-pc-windows-msvc`), Node `24.20.0`, npm `11.12.1`, Tauri `2.11.5` with CLI `2.11.4` and build `2.6.3`, React `19.2.8`, Tiptap `3.31.3`, and the ProseMirror packages locked in `apps/desktop/package.json` and `package-lock.json`. The Tauri runtime observed for the spike is WebView2 `152.0.4191.62` on Windows 11 Pro build `26200`. `rusqlite` `0.40.2` is a bundled development-linking check only; it does not mean V3 has persistence.

The trial surface is intentionally small: type and format sample prose, insert scene breaks, undo/redo, select a passage, keep feedback, preview or reject a local replacement, apply it in the current editor session, and ask Rust to validate the canonical snapshot. Closing the window clears the manuscript and feedback. Use sample text only.
