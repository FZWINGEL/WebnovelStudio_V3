# WebnovelStudio V3

An installed desktop writing application with a Rust core, a project library, free-order story development, and chapter or selected-passage feedback. The design selects Tauri, React/TypeScript, Tiptap/ProseMirror, and SQLite, subject to native editing qualification.

**Current state: architecture and workspace foundation only.** This repository contains design documents and operating rules. There is no Rust application, dependency lock, installer, or V3 test result yet.

Start with the [design index](docs/README.md), then the [delivery plan](docs/V3_FIRST_SLICE_PLAN.md). The [workspace plan](docs/V3_WORKSPACE_PLAN.md) defines repository layout, prerequisites, data separation, and the first implementation handoff. The [architecture](docs/V3_ARCHITECTURE_REFINED.md) specifies document ownership, saving, feedback, recovery, and later story-awareness contracts.

The initial implementation order is a native editor experiment, **A: Writing**, **B: Feedback**, and **C: Release qualification**. Early milestones use synthetic material for author trials; real-manuscript adoption has additional recovery, native, and migration gates. No feature checkbox or mocked response establishes those gates.

V2 remains separately available at `D:\WebnovelStudio_V2`; its source baseline is [c41c6e4](https://github.com/FZWINGEL/WebnovelStudio_V2/tree/c41c6e45c41cfdbcfcf51aa4840605efb4975845). [Migration evidence](docs/V2_MIGRATION_EVIDENCE.md) records verified source contracts. V3 will import a consistent V2 snapshot into a new project; opening V3 will never upgrade or overwrite V2 data.

This is a separate local Git repository on `codex/v3-foundation`. A GitHub remote and application scaffold have not been created. Use this directory as the workspace for subsequent V3 implementation tasks.
