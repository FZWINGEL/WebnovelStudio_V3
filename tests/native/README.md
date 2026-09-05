# Native W0 trial

These checks drive the built Windows Tauri application through its WebView2 remote debugging endpoint. They do not launch a standalone Chromium browser. The smoke flow is implemented by `apps/desktop/scripts/native-smoke.mjs` and writes machine-local output under `.local/native-results/`, which is ignored.

From the repository root, use the wrapper:

```powershell
.\scripts\desktop.ps1 -Command spike
.\scripts\desktop.ps1 -Command native
```

The wrapper prepares the pinned Node `24.20.0` and user Cargo paths. The native flow launches `target/debug/webnovel-desktop.exe`, connects through WebView2's local debugging endpoint, waits for the `Chapter manuscript` textbox, and invokes the real Tauri commands.

The current flow checks:

- Tauri/WebView2 startup and the runtime report, including `persistence: false`;
- editor/Rust canonical JSON and SHA-256 agreement over real IPC;
- selection quotation, feedback composer focus, and editor identity across feedback updates;
- replacement preview and Reject without mutation;
- strict local replacement, surrounding-content preservation, Undo, and Redo;
- internal Unicode edge cases including accents, emoji, combining marks, ZWJ text, and names;
- `Ctrl+Shift+F` selection capture and focus transfer;
- refusal to apply a captured replacement after an intervening manuscript edit;
- a visible manuscript keyboard-focus indicator;
- actual WebView2 `Ctrl+C`/`Ctrl+V` clipboard copy/paste of formatted Unicode paragraphs with emoji and unique block IDs; and
- right-click selection feedback.

The source-level checks cover the same restricted document and identity contract with shared golden fixtures. Native Windows UI Automation exposes the manuscript textbox, feedback labels, and actions. Unicode input is internal robustness evidence for English authoring, not a separate language feature. The remaining English native author trial, minimum-window behavior, external Word paste, and screen-reader user trial are still open.

W0 has no disk-backed manuscript, project/session persistence, provider, durable Apply, receipt, reconciliation, or author-data path. The executable is a development spike and the sample text is cleared when the window closes. Do not use a real manuscript. The [W0 qualification record](../../docs/W0_QUALIFICATION.md) records the current verdict and evidence without turning these checks into a release claim.
