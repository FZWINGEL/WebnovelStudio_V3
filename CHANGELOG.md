# Changelog

All notable user-facing changes are recorded here. This file describes the current development candidate; it does not by itself mark a public release.

## 3.0.0 — Unreleased

WebnovelStudio V3 is a native Windows desktop writing workspace for English web novels, including wuxia, xianxia, cultivation, progression, and translated-register styles.

### Added

- Per-project tabs for Chapters, Worldbuilding, Characters, Plot & Themes, and Notes, with documents available in the order that fits the project.
- AI-first writing flows for drafting, continuing, and developing a story while keeping the author in control of briefs, candidates, feedback, and explicit application of changes.
- A V2-style model picker with provider browsing, search, favorites, keyboard selection, reasoning effort, and fast/service-tier controls.
- Dynamic Codex model discovery without pinning the Codex CLI version, plus the Claude author surface and configurable OpenAI-compatible endpoint profiles.
- A dedicated Story Memory and summary route using GPT-5.6 Luna with xhigh reasoning. The Codex route is the primary development path; Claude and live HTTP behavior remain qualification boundaries.
- Persistent story context with source-linked evidence, reviewed story knowledge, character and promise history, bounded lookup, context inspection, and guarded passage or structured suggestions.
- Safe local persistence, explicit Apply/Reject review, backup and recovered-project flows, V2 import support, and draft export.

### Current verification

- The current candidate has 721 Rust tests and 434 frontend tests in the verified matrix. The hosted CI result is recorded in the release preparation guide.
- Version consistency is checked from the Cargo workspace version across Rust, Tauri, npm, and lock files.

### Still being qualified

- A fresh 3.0.0 Windows package and installer run, including offline and no-WebView2 cases.
- Upgrade retention, native backup/recovery/export dialogs, accessibility and keyboard behavior, high-DPI behavior, long chapters, and interruption recovery on the packaged application.
- Live provider and release qualification. Claude and HTTP integrations are present development surfaces; their full live qualification is not claimed here.
