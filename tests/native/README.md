# Windows Native Execution Runbook

This directory (`tests/native/`) is the implementation home for WebnovelStudio V3 native automated test suites. 

These suites drive the compiled Windows Tauri desktop application through its development-only WebView2 remote debugging endpoint (CDP). They validate real native OS integration: window lifecycle, IPC serialization, SQLite autosave, UI Automation, dialog handling, and crash recovery.

> [!NOTE]
> `apps/desktop/scripts/native-*.mjs` are forwarding stubs retained for compatibility with legacy `npm run test:native-*` scripts. All suite logic, fixtures, and helpers reside here in `tests/native/`.

---

## 1. Prerequisites

- **Operating System**: Windows 10 or 11 (64-bit).
- **WebView2 Runtime**: Microsoft Edge WebView2 Evergreen Runtime installed.
- **Node.js**: `v24.20.0` (managed and pinned via `scripts/desktop.ps1`).
- **Dependencies**: Native harness packages installed in `tests/native`:
  ```powershell
  # From repository root:
  .\scripts\desktop.cmd ensure-native
  # Or directly:
  npm.cmd --prefix tests/native ci --prefer-offline --no-audit
  ```

---

## 2. Execution Workflow

### Step 1: Build the Native Debug Executable
Before running native tests, compile the application binary with the remote debugging endpoint enabled:
```powershell
.\scripts\desktop.cmd spike
```
This builds `target/debug/webnovel-desktop.exe` with development capabilities.

### Step 2: Run Native Suites

#### A. Main Native Smoke Suite
To run the primary editor smoke flow:
```powershell
.\scripts\desktop.cmd native
# Or directly:
node tests/native/native-smoke.mjs
```

#### B. Individual Specialized Suites
Specialized suites test distinct subsystem lifecycles:
```powershell
# Project Conversation (Chat-first) UI smoke:
node tests/native/native-chat-smoke.mjs

# Clean close, dirty buffer prompts, and WM_CLOSE handling:
node tests/native/native-app-close.mjs

# Renderer reload, process crash, and lost-acknowledgment recovery:
node tests/native/native-interruption.mjs

# Backup restoration and project recovery dialogs:
node tests/native/native-recovery.mjs

# Story Workshop multi-candidate authoring:
node tests/native/native-workshop.mjs

# Offline reviewed-memory lookup loop:
node tests/native/native-memory-lookup.mjs
```

#### C. Partitioned CI Execution
To execute suites using the CI consumer partitions:
```powershell
# Create test artifact package:
node scripts/native-artifact.mjs create .local/native-app

# Execute 'main' partition:
node scripts/native-consumer.mjs main .local/native-app

# Execute 'lifecycle' partition:
node scripts/native-consumer.mjs lifecycle .local/native-app

# Reconcile all emitted evidence against scripts/native-suites.json:
node scripts/native-consumer.mjs aggregate .local/native-results .local/native-app
```

---

## 3. Evidence and Output

- All native suites write JSON reports, failure screenshots, and diagnostic logs to `.local/native-results/`.
- The suite manifest at [`scripts/native-suites.json`](../../scripts/native-suites.json) defines the required checkpoint IDs for every suite.
- An execution passes only when the reconciliation step confirms that every checkpoint declared in `scripts/native-suites.json` was emitted with passing status.

---

## 4. Native Troubleshooting

### Orphaned Desktop Processes
If an earlier test run crashed or terminated uncleanly, a dangling `webnovel-desktop.exe` process may hold SQLite locks or occupy the debugging port:
```powershell
Get-Process webnovel-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
```

### Remote Debugging Port Collisions
The native harness configures WebView2 to listen on an ephemeral localhost debugging port. Ensure your local firewall allows localhost loopback connections on high-range TCP ports.

### OS Clipboard Interaction
Certain environments (such as remote desktop sessions without active focus) may restrict Windows clipboard access (`OpenClipboard(NULL)` returning access denied). CI runs in dedicated runner sessions with active desktop focus.

---

## 5. Historical Qualification Checkpoints

The following counts and checkpoint IDs represent historical qualification milestones recorded during development. Current requirements are governed exclusively by `scripts/native-suites.json`.

- **52-Check Checkpoint (Schema 40 / Chat-first & Recovery)**: Main smoke covering character knowledge, accepted summaries, and recovery copy.
- **43-Check Checkpoint (Schema 23 / Structured Suggestions)**: Added block-range and whole-chapter suggestions, covered in [CI 34017484597](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34017484597).
- **41-Check Checkpoint (Schema 22 / C5-A Evidence History)**: Added cross-chapter entity reuse, covered in [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823).
- **40-Check Checkpoint (Schema 21 / Reviewed Story Evidence)**: Added passage-backed review records, covered in [CI 34012813796](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34012813796).
- **38-Check Checkpoint (Schema 20 / Reviewed Chapter Export)**: Added author-reviewed chapter export, covered in [CI 34010306332](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34010306332).
