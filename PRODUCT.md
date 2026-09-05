# WebnovelStudio V3 product requirements

**Status:** confirmed user direction and design baseline, 5 September 2026. Implementation is pending.

The application helps an author create and manage several webnovel projects, develop story material in any order, write chapters, and revise them through discussion. It should feel like a writing application with an assistant.

- **Installed desktop:** an application window, native menus/dialogs, and an installer. Rust owns the core; a web UI embedded in Tauri is the selected design. An externally opened browser application does not satisfy the requirement.
- **Project management:** creating, opening, finding, renaming, duplicating, archiving, switching, and resuming projects are central author flows. Samples are explicitly chosen. A blank project is useful without a model connection.
- **Free creative order:** an author can start with a world, protagonist, theme, hook, scene, chapter, or an ordinary note. These are optional entry points, not required stages or an exhaustive taxonomy.
- **Writing first:** the manuscript is central. Plain language, accessible controls, familiar editing, persistent position, and recoverable saves take precedence over displaying backend workflow stages.
- **Whole-chapter feedback:** a chapter has a persistent chat. After generation, make the feedback composer available; the author supplies feedback and the assistant responds to an explicit request. Creating/saving a chapter does not start a paid critique.
- **Selected feedback:** words, sentences, paragraphs, and larger selections can be discussed through a toolbar, right-click menu, and keyboard alternative. Show exactly what is selected and what an edit may change.
- **Author control:** ordinary explicit edit requests produce reviewable suggestions. Apply changes the manuscript; Reject leaves it alone. Discussion is not canon and never silently authorizes a whole-chapter rewrite.
- **Local ownership:** manual writing works offline; projects are portable and recoverable. A single working manuscript is distinct from revisions, accepted story records, and export/publication history.
- **Explicit model choice:** recognizable model names and separate supported traits remain visible. Setup and credentials belong in Settings. No silent provider/model substitution.
- **Languages and genre:** support English and Chinese editing without making one genre's stages, chapter lengths, or production schedule mandatory.

The user chose a separate repository at `D:\WebnovelStudio_V3`, alongside V2. The [workspace plan](docs/V3_WORKSPACE_PLAN.md) implements that separation. Windows is the first qualification target; other operating systems, collaboration, and cloud sync are deferred until specifically justified.

The design documents convert these requirements into proposed contracts and tests. They do not demonstrate native editing quality, provider reliability, continuity coverage, or literary quality.
