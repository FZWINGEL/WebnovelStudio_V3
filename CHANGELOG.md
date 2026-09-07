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
- Story Workshop relationship exploration for named directional author intentions with exact endpoint heads, plus noncanon moment taste tests that require two or three treatments. Schema 36 raises the reader floor for optional relationship packet context while preserving older Workshop bytes and hashes.
- Optional Unicode names, aliases, and transliterations in the existing `document_aliases` metadata (no schema change) for character/world documents, exposed in the World/People saved-material picker and Writer **Names & aliases** surface. Saving is explicit, source-epoch CAS checked, reconciled by read after uncertainty, and guarded against dirty navigation/close; it leaves title/body unchanged and keeps aliases out of restricted context.

### Current verification

- The W23 aliases slice passes component focused checks (11), Workshop checks (28), and Writer/session checks (42). The 19:22 final wrapper passed 763 Rust tests (70 core unit, 620 grouped integration, 73 desktop; one intentional subprocess ignore), 534 frontend tests in 48 files, 11 tooling checks, formatting, strict Clippy, TypeScript, and production build; the pre-existing large-chunk warning remains. The pinned frontend-only check at 19:24 also passed 534 tests in 48 files after the accessibility markup correction. Evidence is in `.local/workshop-aliases-final-check.log` and `.local/workshop-aliases-final-frontend.log`.
- The source-final aliases headless fixture passed 1440 and 800 pixel checks for Unicode/transliterations, dirty navigation refusal, exactly one lost-ack current-read confirmation, close/reopen, and no AI/manuscript calls, errors, or overflow; report: `.local/workshop-aliases-qa/report.json`. Fresh native aliases CI, upgrade, full specification, and author qualification remain pending.
- CI 34145255173 reached 21 Workshop groups, including relationship exploration and preset/lens/new-project flows, before the W30 sidebar-close harness failure; later auxiliary suites were skipped. The native harness now contains names/aliases checks, with fresh CI 34147701184 and installer run 34147720364 now in progress on `b801ae9`.
- Version consistency is checked from the Cargo workspace version across Rust, Tauri, npm, and lock files.
- The 3.0.0 installer passes synthetic writing, save/reopen, normal close, and same-version uninstall/reinstall with retained text. Exact artifacts and source identities are recorded in the [release preparation guide](docs/RELEASE_3_0_0.md).

### Still being qualified

- Offline installation and the no-WebView2 case.
- Upgrade retention, native backup/recovery/export dialogs, accessibility and keyboard behavior, high-DPI behavior, long chapters, and interruption recovery on the packaged application.
- Live provider and release qualification. Claude and HTTP integrations are present development surfaces; their full live qualification is not claimed here.
