# W0 editor trial design

This document records the visual surface that is built in `apps/desktop/src/shell/app.css` and `App.tsx`. It describes the current trial; it is not a new product identity exercise. The companion [editor trial brief](docs/EDITOR_TRIAL_BRIEF.md) supplies the short direction brief.

## Surface

The window uses a neutral white manuscript canvas, slate navigation/status chrome, and muted blue actions. The header carries the authored SVG book mark, WebnovelStudio name, an `Editor trial` label, and `Sample text · session only`. The main area is split between a wide manuscript column and a fixed 354px feedback panel. The bottom status line says `Manuscript · not saved`, reports local/no-provider state, and shows the WebView2 runtime when native IPC is available.

The manuscript uses a Georgia style face with system fallback for English prose and Segoe UI for controls and headings. The interface uses Segoe UI Variable/Segoe UI. The writing column includes a compact paragraph-style selector, bold and italic controls, scene-break insertion, undo/redo, and selection feedback. The editor is a Tiptap contenteditable exposed as the `Chapter manuscript` multiline textbox; keyboard focus receives a 2px accent outline with a 14px offset so the focused writing surface remains visible against the canvas.

The feedback panel offers Whole chapter and Selected passage scopes. A captured selection shows its quotation and whether it stays within a paragraph or crosses paragraphs. The composer receives focus after selection capture. A local replacement is previewed as Before/After and can be rejected or applied for the current session. The panel labels the lack of an AI response and keeps the sample-only/session-only boundary visible.

Selection feedback is available beside formatting, through the context menu, and with `Ctrl+Shift+F`. Status and error text use live regions; buttons, scopes, the composer, and the manuscript expose accessible names. Narrow windows reduce the panel and manuscript padding while retaining the two-column writing arrangement.

## Editorial constraints

The product targets English novel authoring, UI, and export. This W0 surface exercises typing, selection, captured feedback, local replacement preview, application, and undo; export is later work. Optional translated-webnovel, wuxia, or xianxia register and terminology may shape later style work; they are not required genres. The surface does not expose projects, model setup, persistence, provider state, or story workflow stages because those are not implemented in W0. Internal Unicode fixtures exercise accents, emoji, and names for robustness.

The SVG icon is authored in `apps/desktop/src-tauri/icons/icon.svg`; the Windows ICO is generated from that source by the Tauri tooling. No external raster asset or unproven brand source is used.

An independent finish review accepted the desktop, selection-preview, and focused-editor captures after the manuscript focus indicator was added. The bundled reviewer role was unavailable, so a separate review agent followed the same scoped checklist. This verdict covers the W0 surface only. The English native author trial, minimum-size/DPI behavior, external Word paste, native installer presentation, and screen-reader usability remain qualification work.

## Saved versions in the persistent workspace

The W6 history surface extends the existing writing desk using the same white/slate palette, muted blue actions, Segoe UI controls, and Georgia prose. **History** beside the document title opens a dated version selector and an inert comparison panel. The current manuscript stays visible and mounted. **Restore this version** sits below the preview with an explicit statement that the current writing remains in history. Loading, failed reads, identical versions, and pending restore have distinct states. See the [surface brief](docs/HISTORY_SURFACE_BRIEF.md) and [restore contract](docs/ADR_0005_DOCUMENT_HISTORY.md).

An independent finish review accepted the native comparison and post-restore captures after ownership guards and pending-action controls were fixed. The additional 800×600 CSS viewport check kept text and actions reachable without horizontal overflow, though the three-column layout is cramped. That check used viewport emulation inside WebView2; native window resizing, DPI behavior, keyboard navigation, and screen-reader qualification remain open. No raster assets were introduced.

## Draft export in the persistent workspace

W7 adds a modal **Export draft** surface using the existing white/slate controls and muted blue actions. It names the document and saved version, offers Markdown or plain text, and shows the exact inert output in a scrollable preview. Formatting limitations and omissions appear beside the format choice. **Choose destination…** opens the native Save dialog. Cancel retains the preview; a successful save shows its destination and **Done**. Possible writes with missing receipts are disclosed before another export is prepared. The underlying editor stays mounted, and closing returns focus to Export draft.

The independent finish review accepted default-size and 800×600 CSS-viewport captures: the format, note, preview, and actions remained contained and reachable, with no horizontal overflow. It found noisy escaping of ordinary periods; the projection was corrected while preserving literal numbered lines. This is WebView2 viewport evidence, not native resizing, DPI, or screen-reader qualification. The real Cancel flow was exercised separately; local Windows automation could not complete Save, so completed-export presentation and the full native Save journey await hosted qualification. No raster assets were introduced. See the [export brief](docs/EXPORT_SURFACE_BRIEF.md) and [export contract](docs/ADR_0006_DRAFT_EXPORT.md).
