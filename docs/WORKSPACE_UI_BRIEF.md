# W3 workspace UI brief

**Status:** implementation brief for the planned W3 native library and durable writing surface. It is not a shipped-status report. The current runtime remains the W0 sample editor trial.

**Scope:** English novel authoring, UI, and export. Wuxia, xianxia, cultivation, and translated-webnovel register or terminology may be optional English style support; no language or genre pipeline is mandatory. Use the existing neutral white, slate, and muted blue surface with Segoe UI controls and Georgia prose. Do not introduce new branding.

## Library and project entry

The first native workspace is an empty Library when no projects exist. Its useful actions are **New project** and **Open folder**; search and archive filters remain available as the library grows. A missing path offers **Locate** rather than silently creating an empty replacement. Removing a library entry does not delete its project folder.

**New project** asks only for a title; `Untitled project` is valid. The author may immediately create an optional document of type chapter, character, world, theme, hook, scene, or note. These types provide labels and sensible defaults, not required forms or a fixed order. A new project and a blank document make no provider request. The W0 sample is an explicitly chosen trial surface and is never the default project or a hidden seed.

## Writing workspace

The workspace has a collapsible document sidebar, a central persistent editor, and room for later feedback. The sidebar lists the project's documents and supports search, selection, and resume of the last item. The central editor owns the current document body and caret. A chapter owns its prose; scenes are structural breaks or explicitly copied scene drafts. The assistant pane is introduced with W4 discussion and jobs, so W3 does not show a fake AI pane or imply a connected model.

The editor keeps the current body mounted while unrelated shell state changes. Its author-facing labels are plain: `Saving…`, `Saved`, and `Couldn't save`. `Saved` means the visible generation has a successful Rust acknowledgment for a SQLite commit. A scheduled or uncertain save does not receive the Saved label. On a definite save error, keep the live buffer visible and offer Retry, Save recovery copy, and Copy text. On an uncertain outcome, reconcile before another write and explain that state.

## Switching, close, backup, and export

Switching documents or projects joins the current lifecycle guard, lets composition finish, drains saves, stores caret/composer state, and only then disposes the editor. A normal application close follows the same flush guard and gives an explicit choice for running jobs or unsaved work. A late callback carries its captured project/document/session identity and cannot write into the destination project.

The project owns a portable folder and one authoritative SQLite database. **Backup** opens a native destination dialog, creates a consistent staged archive, verifies its manifest and referenced files, and finalizes it only after validation. **Recover** validates into a new staging folder, assigns a new project identity and operation namespace, and opens the recovered copy after completion. The original project remains untouched if recovery fails; recovery never overwrites an open project. A backup on the same drive is described as protection from mistakes, not drive loss.

The first export is an explicit UTF-8 plain-text **Export draft** action from a flushed, frozen current source. It explains that rich formatting, comments, and history are not represented. Exporting does not mark prose ready, establish continuity, or publish it. Markdown and stronger reviewed-story exports remain later gates.

## Accessibility and failure acceptance

- Every library action, document row, editor, status, dialog, and destructive choice has a stable accessible name and keyboard path; focus is visible and returns to the invoking control after a dialog or project switch.
- Empty Library, empty project, missing folder, archived-only filter, and no-search-results states explain the next action without inventing sample content.
- Save status is text and live-region announced; error text identifies the affected project/document and offers a recoverable next action. Unsaved text stays available in the editor.
- Native folder, backup, recovery, and confirmation dialogs support Escape, Enter, keyboard traversal, cancellation, and an explicit destructive confirmation. A failed restore never lists staging as a completed project.
- A conflict presents the current saved body and the unsaved/local body together with an explicit recovery choice. It never resolves divergence by silently choosing last-write-wins or remounting over newer local text.
- The writing canvas remains usable offline and without model configuration. The design does not rely on color alone, and the manuscript focus ring remains visible against the white canvas.

Implementation should be checked against [PRODUCT.md](../PRODUCT.md), the built visual constraints in [DESIGN.md](../DESIGN.md), lifecycle and recovery contract §8 of [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md), and the W3 package in [V3_FIRST_SLICE_PLAN.md](V3_FIRST_SLICE_PLAN.md). W3 is complete only after the native library, file-backed persistence, flush/recovery behavior, independent-copy restore, and draft export have their own executed evidence.
