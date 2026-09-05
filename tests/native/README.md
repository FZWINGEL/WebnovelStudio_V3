# Native development smoke

These checks drive the built Windows Tauri application through its development-only WebView2 remote debugging endpoint. They do not launch a standalone Chromium browser. The smoke flow is implemented by `apps/desktop/scripts/native-smoke.mjs` and writes ignored output under `.local/native-results/`. The separate local diagnostic subset intentionally omits the currently blocked OS clipboard step and labels that omission; it cannot establish a strict pass.

From the repository root, use the wrapper:

```powershell
.\scripts\desktop.ps1 -Command spike
.\scripts\desktop.ps1 -Command native
```

The wrapper prepares the pinned Node `24.20.0` and user Cargo paths. The native flow launches `target/debug/webnovel-desktop.exe`, connects through WebView2's local debugging endpoint, waits for the persistent Library/Workspace UI, and invokes the real Tauri commands. The explicit W0 sample editor remains a separate session-only trial.

The current development flow checks:

- Tauri/WebView2 startup and the runtime report, including `persistence: true`;
- persistent blank-project Library/Workspace startup and optional document creation;
- Rust SQLite autosave and flush-before-switch behavior;
- project/document rename, duplicate, archive/unarchive, and native folder/backup/recovery/draft-TXT dialog wiring;
- editor/Rust canonical JSON and SHA-256 agreement over real IPC;
- selection quotation, feedback composer focus, and editor identity across feedback updates;
- replacement preview and Reject without mutation;
- strict local replacement, surrounding-content preservation, Undo, and Redo;
- internal Unicode edge cases including accents, emoji, combining marks, ZWJ text, and names;
- `Ctrl+Shift+F` selection capture and focus transfer;
- refusal to apply a captured replacement after an intervening manuscript edit;
- a visible manuscript keyboard-focus indicator;
- actual WebView2 `Ctrl+C`/`Ctrl+V` clipboard copy/paste of formatted Unicode paragraphs with emoji and unique block IDs; and
- right-click selection feedback;
- persistent author-room discussion with exact selected quotations and a deterministic mock response;
- prepared/delivered context receipt inspection over native IPC;
- persistent document/project discussion sources, explicit inspector confirmation, required-source labels, and retention after navigation/reload;
- optional writing-brief adaptation and explicit approval, draft reload, exact supplied wording, exclusion of original private planning, and receipt inspection before Apply;
- draft and discussion-history retention across project switching and renderer reload;
- W5 selected-passage `ProposeEdits` review cards, edited preview retention across navigation/reload, exact Apply with protected ending, stale pending alternatives, Reject without change, and undo/redo body and decision retention after reload in the diagnostic subset; and
- W6 saved-version History comparison, explicit document restore, lost-acknowledgment recovery, and undo/redo/reopen behavior;
- keyboard Undo/Redo while a real background reading-position acknowledgment is delayed, with the manuscript remaining focused and editable;
- W7 exact Markdown/TXT preview, real native Save-dialog cancellation, a real file whose bytes match the preview, unchanged editor state, and focus restoration; and
- strict full-flow clipboard copy/paste coverage, which remains part of the 26-check smoke and is not omitted from CI.

The source-level checks cover the same restricted document and identity contract with shared golden fixtures. Current schema 11 includes writing-brief drafts, persistent discussion source choices, guidance, durable discussion retry, proposals, and export records. Native Windows UI Automation exposes the manuscript textbox, feedback labels, discussion controls, guidance form/actions, context inspector, and proposal cards. Unicode input is internal robustness evidence for English authoring, not a separate language feature.

The W7 dialog helper is bounded to the spawned test application's PID and a new output inside its synthetic temporary directory. It uses Windows UI Automation for the actual dialog. Its process-scoped PowerShell execution policy does not change machine policy. No fake export IPC response or author filesystem path is used. The lost-acknowledgment and delayed-view injections live in the external development harness; they are not compiled into application code. The earlier UIA focus route failed; the current owned-dialog helper uses `WM_NEXTDLGCTL`, `EM_SETSEL`, and `EM_REPLACESEL`, verifies exact readback, sends no global keystrokes, and closes ExportDialog before restoring parent-window focus.

Historical W4/C3/retry development flows passed 16–19 checks; the W5 diagnostic subset passed 20 and W6 passed 21. A fresh diagnostic built from pushed commit `78367cd5d359b573daa32be1a32a8d6038520508` passed 23 checks on actual Tauri/WebView2 `152.0.4191.62`, omitting only local clipboard coverage. It is development evidence, not a strict/full-native pass. The added C3 source-choice flow passed 24 local diagnostic checks on the same WebView2 version, again omitting only clipboard. Current native runs must use an executable rebuilt from the current frontend and Rust source.

The writing-brief development build passed **25 diagnostic checks** on WebView2 `152.0.4191.62` at `2026-09-05T20:45:34.100Z`. Its new flow adapts a private planning message, requires approval, preserves the brief after navigation/reload, verifies the exact delivered wording and excluded origin text, and inspects the receipt before any Apply. Only the known local clipboard check is omitted. The previous source-choice checkpoint `16bf8bc` passed all **25 strict checks**, including clipboard, in [CI run 33990073110](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990073110) on WebView2 `151.0.4129.101`; it does not qualify the newer brief flow.

The strict local flow currently stops at Ctrl+V: copy serializes correctly, but WebView2 receives empty clipboard data. The same issue occurred with an older binary. A separate Win32 probe received access denied from `OpenClipboard(NULL)` in all 20 attempts and found no owner window. The cause is unresolved; no clipboard service was restarted and no clipboard contents were inspected. The diagnostic subset omits only this step and writes a separate report. Hosted Windows CI still runs the unchanged strict clipboard check.

The pushed W5 checkpoint [`a2a0163`](https://github.com/FZWINGEL/WebnovelStudio_V3/commit/a2a01632890a44efbe84bc526674fc9bf06d3d94) is covered by [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728), whose Windows-native strict21 job is green. The export-dialog/native-save correction is now pushed as [`78367cd`](https://github.com/FZWINGEL/WebnovelStudio_V3/commit/78367cd5d359b573daa32be1a32a8d6038520508); [CI run 33988660050](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33988660050) for `78367cd` passed Windows/Ubuntu contracts and the strict 24-check native flow on WebView2 `151.0.4129.101`, including clipboard, history recovery, exact chosen-path export, and focus restoration. The earlier package-source native attempt [`4a59a19`](https://github.com/FZWINGEL/WebnovelStudio_V3/commit/4a59a19) failed at `UIASetFocus`. Current results and fresh CI are maintained in [implementation status](../../docs/IMPLEMENTATION_STATUS.md). This flow remains mock-only; bounded live experiments are recorded separately in [Codex qualification](../../docs/CODEX_QUALIFICATION.md).

The English native author trial, physical minimum-window/DPI behavior, external Word paste, screen-reader use, backup/recovery dialog journey, and installed-release qualification remain open. The local export diagnostic covered exact Markdown/TXT preview, native Save cancellation, chosen-path byte equality, unchanged editor state, and focus restoration; it omitted only local clipboard coverage and does not establish strict/full-native qualification. An 800×600 CSS viewport capture is useful layout evidence but does not establish native resizing or multi-DPI behavior. See [Windows package qualification](../../docs/WINDOWS_PACKAGE_QUALIFICATION.md) for the separate release path.

The W0 sample trial has no disk-backed manuscript, provider, durable Apply, or author-data path; its sample text is cleared when the window closes. The default Library/Workspace has disk-backed project persistence, but this development flow is not an installed-release or native-qualification claim. Do not use a real manuscript for the W0 sample trial. The [W0 qualification record](../../docs/W0_QUALIFICATION.md) records the spike verdict and evidence without turning these checks into a release claim.
