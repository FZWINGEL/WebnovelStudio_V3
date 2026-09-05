# W0 execution and qualification

**5 September 2026. Verdict: the native editor spike is implemented and its automated checks pass; W0 remains in progress until the remaining native author trials are recorded.** This is a development experiment, not the Writing milestone or a production release.

The author clarified that the product is for English novels, optionally using wuxia/translated-Chinese-webnovel style. The visible sample and UI are English. Chinese input/Pinyin qualification is not required; non-Latin test strings remain synthetic Unicode edge cases.

## Built boundary

The repository contains two Rust crates and one frontend package. The Windows Tauri executable embeds the built React/Tiptap editor and opens its own native window. The automated trial confirmed its actual URL is `http://tauri.localhost/`, with no Vite/Node application server running. Rust and JavaScript independently agree on canonical JSON and SHA-256 over real IPC. Rust validates a restricted snapshot; it does not store a manuscript or independently validate replacement scope yet.

The author can type and format sample prose, insert a scene break, capture a word/sentence/multiple-paragraph selection, keep chapter or passage feedback for the session, preview a manually written replacement, reject it, apply it locally, and undo/redo. Toolbar, context menu, and keyboard routes reach selection feedback. No provider request is made. Closing the window clears all prose and feedback.

See [ADR 0001](ADR_0001_EDITOR_CONTRACT.md) for the representation and [the delivery plan](V3_FIRST_SLICE_PLAN.md) for W1 onward. Project library, file-backed storage, save/reconciliation, independent Rust scope validation, durable Apply, providers, and import are not implemented by this slice.

## Reproducible local results

Commands run from the V3 repository, using `scripts/desktop.ps1`:

| Check | Result and scope |
| --- | --- |
| `-Command check` | Passed: workspace rustfmt, workspace clippy with warnings denied, workspace Rust tests, TypeScript check, production frontend build, and frontend tests. |
| Rust tests | 6 core tests passed. One traverses all 13 shared snapshot fixtures; others check SQLite linking, unknown fields, raw-size, block-count, and UTF-16 limits. The thin host has no unit tests; real command integration is covered below. |
| Frontend tests | 21 passed. Golden snapshots, safe links, paragraph/heading splits at start/middle/end, left identity, merge/undo/redo, copied-block insertion/undo, surrogate/combining/ZWJ snapping, exact repeated occurrence, formatting preservation, stale-after-undo, and refused structural replacements. |
| `-Command spike` | Passed: Windows MSVC debug executable built with embedded local frontend assets and locked Cargo dependencies. |
| `-Command native` | 10 checks passed in the actual Tauri/WebView2 application; no standalone Chromium browser and no mocked Rust invocation. |
| `-Command build` | Optimized Windows executable built successfully, without an installer. This build result is not a release qualification. |
| Native accessibility inspection | Windows UI Automation exposes the manuscript textbox, feedback labels/actions, headings, formatting controls, and native title-bar controls. This is tree inspection, not a screen-reader user trial. |
| Interface review | Independent review accepted desktop, selection preview, and focused-editor captures after a visible editor focus outline was added. The Impeccable mechanical detector returned no findings. |
| V2 boundary | V2 stayed clean at `c41c6e45c41cfdbcfcf51aa4840605efb4975845`. No V2 source, author database, or Git remote was changed. |

The 10 native checks cover actual host/runtime reporting; editor/Rust fingerprint agreement; captured quotation and composer focus with stable editor identity across feedback updates; nonmutating Preview/Reject and local Apply/Undo/Redo preserving surrounding nodes; visible keyboard focus; programmatic Chinese/emoji/combining/ZWJ input; `Ctrl+Shift+F`; stale-scope refusal; actual formatted `Ctrl+C`/`Ctrl+V` clipboard round-trip; and right-click selection feedback.

The most recent machine-local report is `.local/native-results/report.json`. Screenshots are `desktop.png`, `selection-preview.png`, and `editor-focused.png` in that directory. These generated outputs are ignored; rerun the native command to reproduce them. Native tests use unique temporary WebView data directories and a child-process-only loopback debugging port. Ordinary trial launches use a checkout-specific directory beneath `%LOCALAPPDATA%\WebnovelStudioV3-Dev`. No author project directory is opened.

The frontend build emits a warning for its approximately 502 kB JavaScript entry chunk (about 155 kB gzip). It builds successfully. No typing-latency or large-manuscript performance qualification is inferred from that size or the fast sample trial; E2 remains separate.

## Locked dependencies and observed host

| Component | Qualified build selection or observed value |
| --- | --- |
| Rust / target | `1.98.1`, `x86_64-pc-windows-msvc`; rustfmt/clippy in `rust-toolchain.toml` |
| Node / npm | Node `24.20.0`; npm `11.12.1` used for installation. Wrapper selects Node through npm's managed cache without changing V2 or the global Node installation. |
| Tauri | Rust `2.11.5`, build helper `2.6.3`, CLI `2.11.4`, API `2.11.1` |
| React / Tiptap | React/React DOM `19.2.8`, Tiptap core/react/pm `3.31.3` |
| ProseMirror | model `1.25.11`, state `1.4.4`, transform `1.12.1`, view `1.42.3`; full transitive versions in the npm lockfile |
| Frontend build/tests | TypeScript `7.0.2`, Vite `8.2.2`, Vitest `5.0.0`, Playwright core `1.63.0` for native CDP checks |
| SQLite | `rusqlite 0.40.2`, `libsqlite3-sys 0.38.2`, bundled SQLite `3.53.2`. This is a core test dependency and linking probe; the app has no SQLite persistence path. |
| Windows | Windows 11 Pro, 64-bit, build `10.0.26200` |
| Native compiler / SDK | Visual Studio Build Tools `18.4.11626.88`; installed/default MSVC tools `14.50.35717`; Windows SDK `10.0.26100.0`. Real builds linked successfully. These machine-installed components are recorded, not managed by Cargo's lockfile. |
| Running WebView2 | `152.0.4191.62`, queried from the actual Tauri host |

Cargo and npm lockfiles are committed. Machine compiler, SDK, and evergreen WebView2 upgrades require rerunning the native qualification; dependency pins alone cannot freeze those installed components.

The toolchain setup followed [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) and [Rust's installation guidance](https://rust-lang.org/tools/install/). Rustup was downloaded from the official distribution and verified against its published SHA-256. Native automation attaches to this app's WebView2 through [Playwright's CDP connection](https://playwright.dev/docs/api/class-browsertype#browser-type-connect-over-cdp); this does not turn programmatic input into physical IME evidence.

## Failures resolved in this slice

- Tiptap's heading split at offset zero changed the left block to a paragraph with default attributes, dropping its ID. The narrow split wrapper restores the left ID within the same transaction; start/middle/end and undo/redo tests now pass.
- Hard breaks initially contributed zero to Rust's UTF-16 count. They now contribute one, matching the document contract and corrected shared fixture.
- Link validation initially differed between JavaScript and Rust, and encoded mailto control text could pass. The constrained policy and fixture tests now agree; percent-encoded mailto addresses are refused in W0.
- Unsupported blockquote/code paste formatting initially lacked a notice. The visible conversion notice includes these structures.
- The first native keyboard shortcut check raced Tiptap's scheduled focus. The harness now observes actual editor focus before sending the shortcut. No arbitrary delay was substituted in the regression path.
- The editor initially removed its focus outline without a replacement. A visible focus indicator is now asserted and captured in the actual native runtime.
- A rebuild while the trial executable was still open hit Windows' executable file lock. Closing the owned sample window resolved it; the README states this build requirement.

## Remaining native author trial

Use sample text in the actual executable. Record Windows, WebView2, keyboard layout, input method, DPI, and precise steps for failures.

- English text input: punctuation, accented or transliterated names, emoji, and supported dead-key/composition sequences. Try selection feedback and Apply during active composition; no partially entered character should be applied or discarded. Chinese/Pinyin input is outside the product acceptance scope.
- Clipboard from real external author tools: paste formatted Word/browser paragraphs and unsupported structures. The automated in-app formatted clipboard test passes; external Word compatibility is still unqualified.
- Native keyboard and selection: mouse/Shift+arrow across formatted paragraphs and a scene break, keyboard context menu, focus transfer and Escape return, undo across nearby manual typing and replacement. Several paths pass automation; a physical keyboard trial remains distinct.
- Window and assistive technology: the minimum 800×600 window, resizing, 100/150/200% scaling, focus after switching apps, and a screen-reader reading/editing session. A native resize attempt did not establish a successful minimum-size trial and is not counted as evidence.

These checks are the remaining W0 suitability gate. E2's 20k/250k snapshot/persistence cost, installed release behavior, providers, and narrative quality have their own later gates. Do not mark them green from this spike.

## Repository and CI status

Implementation is local on `codex/v3-native-editor-spike`. A real CI definition now has Linux/Windows core-and-renderer checks and a separate Windows native lane. V3 still has no GitHub remote, so no V3 GitHub Actions run is claimed. V2's earlier CI remains V2 evidence only.

The scoped interface review used an independent review agent because the bundled Impeccable finish-reviewer role was unavailable. Its single required focus fix is resolved. The skill also reported that the existing product record lacks its newer optional metadata format; Impeccable `init` can update that format if requested. Confirmed product requirements were retained.
