# WebnovelStudio V3 product requirements

**Status:** confirmed product direction; W0 native implementation in progress, 5 September 2026.

WebnovelStudio helps an author manage several webnovel projects, develop story material in any order, write chapters, and revise them through discussion. It should feel like a writing application with an assistant.

- **Installed desktop:** a native application window with native menus/dialogs and an installer. Rust owns the core and the web UI runs inside Tauri. An externally opened browser does not satisfy this requirement.
- **Project management:** creating, opening, finding, renaming, duplicating, archiving, switching, and resuming projects are central flows. Samples are explicitly chosen; a blank project remains useful without a model connection.
- **Free creative order:** an author may begin with a world, protagonist, theme, hook, scene, chapter, or ordinary note. These are optional entry points, not required stages.
- **Writing first:** the manuscript is central. Plain language, familiar editing, persistent position, and recoverable saves take precedence over exposing backend workflow stages.
- **Whole-chapter feedback:** a chapter can have a persistent chat. Feedback is requested explicitly; creating or saving a chapter does not start a paid critique.
- **Selected feedback:** words, sentences, paragraphs, and larger selections can be discussed through a toolbar, context menu, and keyboard alternative. The UI shows the captured quotation and the possible edit scope.
- **Author control:** an explicit edit request produces a reviewable suggestion. Apply changes the manuscript; Reject leaves it unchanged. Discussion is not canon and never silently authorizes a whole-chapter rewrite.
- **Local ownership:** manual writing works offline; projects are portable and recoverable. A working manuscript is distinct from revisions, accepted story records, and export history.
- **Explicit model choice:** recognizable model names and supported traits remain visible. Settings own credentials and configuration. There is no silent provider or model substitution.
- **Language and genre:** English is the authoring, UI, and export language. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; Chinese authoring is not a product requirement, and no genre's stages, chapter lengths, or schedule are mandatory.

## Current W0 surface

The implemented W0 surface is a native Tauri/WebView2 editor trial. It opens a built-in sample chapter, supports paragraph and heading styles, bold and italic marks, links, scene breaks, clipboard text/formatting notice, undo/redo, whole-chapter or selected-passage feedback notes, replacement preview/reject, a local replacement transaction, and Rust snapshot validation over IPC. The editor stays mounted while feedback state changes, and stale captured selections are refused after intervening edits.

The trial is not a product release or an author-data path. Text and feedback are session-only and disappear when the window closes. No provider or AI response is connected, and the current Rust command validates snapshots without persisting them. The remaining English native author trial, minimum-window behavior, external Word paste, screen-reader use, disk-backed project recovery, live providers, and installed-package qualification remain future gates. Unicode edge cases such as accents, emoji, and names are internal robustness fixtures, not a Chinese authoring feature. See the [W0 qualification record](docs/W0_QUALIFICATION.md) for executed evidence.

V3 lives in `D:\WebnovelStudio_V3`, alongside V2. Windows is the first qualification target; other operating systems, collaboration, cloud sync, V2 import, and reviewed-story features remain separately gated. The design documents describe intended contracts; they do not establish native editing quality, provider reliability, continuity coverage, or literary quality.
