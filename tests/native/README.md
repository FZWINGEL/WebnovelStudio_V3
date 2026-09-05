# Native development smoke

These checks drive the built Windows Tauri application through its WebView2 remote debugging endpoint. They do not launch a standalone Chromium browser. The smoke flow is implemented by `apps/desktop/scripts/native-smoke.mjs` and writes machine-local output under `.local/native-results/`, which is ignored.

From the repository root, use the wrapper:

```powershell
.\scripts\desktop.ps1 -Command spike
.\scripts\desktop.ps1 -Command native
```

The wrapper prepares the pinned Node `24.20.0` and user Cargo paths. The native flow launches `target/debug/webnovel-desktop.exe`, connects through WebView2's local debugging endpoint, waits for the persistent Library/Workspace UI, and invokes the real Tauri commands. The explicit W0 sample editor remains a separate session-only trial.

The current flow checks:

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
- prepared/delivered context receipt inspection over native IPC; and
- draft and discussion-history retention across project switching and renderer reload.

The source-level checks cover the same restricted document and identity contract with shared golden fixtures. W1 structural scope validation and W2 session/core receipt and reconciliation paths are implemented; current schema-7 storage includes guidance and the saved discussion retry link, exact-head caret/last-document state, metadata compare-and-swap, and `context_source_epoch`. Native Windows UI Automation exposes the manuscript textbox, feedback labels, discussion controls, guidance form/actions, and context inspector. Unicode input is internal robustness evidence for English authoring, not a separate language feature. The remaining English native author trial, minimum-window behavior, external Word paste, and screen-reader user trial are still open.

After a fresh isolated rebuild, the prior W4 real Tauri/WebView2 flow passed 16/16 checks on WebView2 `152.0.4191.62`, including two-project save/switch/reload and copy isolation, process kill/restart, project/document rename, exact caret/last-document restore, archive/unarchive, persistent discussion, selected feedback, context inspection, and draft/history retention. The C3 guidance checkpoint then passed 17/17 checks on the same WebView2 runtime. The current recent-discussion slice passes 18/18 after a fresh rebuild, adding exact prior-exchange and selected-scope inspection to the guidance persistence, policy, and receipt checks. The `discussion-context.png` capture was visually inspected: the prior exchange and selected quotation are readable beside the unchanged manuscript. The current retry slice passed 19/19 after a fresh rebuild, exercising Stop, preparation, navigation/reload, and exact reused instruction inspection. The `discussion-retry.png` capture shows the retained retry notice and composer alongside unchanged prose. This remains development evidence rather than release qualification. Native backup/export/recovery dialog use, packaged installation, accessibility, and the broader A, N, and W3 qualification remain open.

The W0 sample trial has no disk-backed manuscript, provider, durable Apply, or author-data path; its sample text is cleared when the window closes. The default Library/Workspace has disk-backed project persistence, but this development flow is not an installed-release or native-qualification claim. Do not use a real manuscript for the W0 sample trial. The [W0 qualification record](../../docs/W0_QUALIFICATION.md) records the spike verdict and evidence without turning these checks into a release claim.
