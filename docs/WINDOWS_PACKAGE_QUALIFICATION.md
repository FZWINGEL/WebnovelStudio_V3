# Windows package qualification

The initial distribution target is Windows x64 with an NSIS installer. Package configuration is implementation work; a successful build alone does not establish offline installation, accessibility, recovery, or author-trial acceptance. Current executed evidence is recorded in [implementation status](IMPLEMENTATION_STATUS.md).

## Package contract

- Product: **WebnovelStudio V3**; stable bundle identifier: `com.webnovelstudio.v3`.
- The installer targets the current Windows user and uses English setup text.
- `offlineInstaller` embeds Microsoft's Evergreen WebView2 installer so setup does not need to download a runtime at installation time. Building the package may download bundling tools and that runtime. This follows [Tauri's Windows installer options](https://v2.tauri.app/distribute/windows-installer/).
- Release builds use Tauri's `app_local_data_dir()`: `%LOCALAPPDATA%\com.webnovelstudio.v3` on Windows. This path does not include the compilation checkout and remains stable across rebuilds with the same identifier.
- Debug builds retain `%LOCALAPPDATA%\WebnovelStudioV3-Dev\<checkout-hash>` and display **Development** in the native title. Their synthetic data/WebView/CDP overrides remain compiled only with debug assertions.
- There is no automatic transfer of development libraries into release storage. Projects can be explicitly opened through the application's existing native flow.
- The application requires neither a Node server nor a browser launch. Local assets run in the installed Tauri window. Manual writing does not require model discovery or login.

Build with the pinned wrapper:

```powershell
.\scripts\desktop.ps1 -Command package
```

This requests `x86_64-pc-windows-msvc`, NSIS, and the locked Cargo dependencies. `build` retains the release executable without bundling; `spike` and `native` retain the development qualification path. No signing identity or automatic updater is configured in this checkpoint.

## Executed build, 5 September 2026

The pinned `package` wrapper completed successfully on Windows and produced `WebnovelStudio V3_0.1.0_x64-setup.exe` (265,903,797 bytes). The source was the W7 development tree based on `1a6beb7`; the later Markdown punctuation, Windows-basename, and legacy-export removal corrections are not included in this artifact. A fresh package must be built before qualifying those corrections or later provider code. The installer is unsigned.

| Artifact | SHA-256 |
| --- | --- |
| V3 installer | `54fa17f65dee3894869624d73ddd068abfc6ada09e9d193bf7b14d7f46f7ba39` |
| Cargo.lock | `f9c30fe485b2957dd656882a3fb3e92085006004d403f3cabf2553e8c62b6226` |
| Desktop package-lock.json | `f0c20e859a7180b26019e2b6fa20876a3e43881d1921b73b1500f5b3cf21662c` |
| Bundled Microsoft WebView2 installer | `e7fa35755196ad9223596ef021a1ce6799509142eaa40ba35f634026be50b831` |

The bundled Microsoft installer is 258,614,480 bytes; its file/product version is `1.3.265.7`, and Windows verified its Microsoft Corporation signature. This is the installer executable's version, not evidence of the runtime ultimately installed. NSIS 3.11 and `nsis_tauri_utils` 0.5.3 were used by the bundler.

Inspection of the generated NSIS script confirms a default installation under `%LOCALAPPDATA%\WebnovelStudio V3`, separate from `%LOCALAPPDATA%\com.webnovelstudio.v3` data. The uninstaller removes app data only when its **Delete app data** checkbox is selected and the operation is not an update. This source inspection is distinct from an executed uninstall/reinstall trial.

An isolated Windows Sandbox trial was attempted at 18:47 UTC with networking and clipboard redirection disabled, read-only installer input, and a writable synthetic output folder. Windows reported a lost Sandbox connection before any harness output appeared. One explicit reconnect produced the same warning at 18:50:54 UTC. The owned Sandbox client was then closed; its processes exited. No installation, native writing, or uninstall/reinstall result was obtained. No Windows feature or service was changed, and no host author-data directory was exposed to the sandbox. The ignored `.local/package-qualification/` folder retains the configuration, harness, screenshot, and dated reconnect observation for another nominated environment.

## Hosted lifecycle checkpoint

[Run 33988758827](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33988758827) built and installed source `0e3f748e6bfac6570b9bed501b4d00908f3da067` with installer SHA-256 `54ed850a1dcdd2da00bc3c994a9ba451481869c31f44a38ceff70cc0223d0bbc`. The Windows Server 2025 runner (`10.0.26100`, x64) used WebView2 `151.0.4129.101`. Installation returned 0, the release Library hid the editor trial, and synthetic project/document creation and editor text readback succeeded. The flow then timed out waiting for Library after **All projects**. Normal close and uninstall/reinstall were not reached; cleanup was forced. This is a failed lifecycle run, not an installed-release pass.

The harness reacquires the owned native UIAutomation window during navigation waits and captures bounded owned-window failure diagnostics. A separate debug UIAutomation reproduction on WebView2 `152.0.4191.62` exposed a definite project-opener defect: the regex overwrote the helper's `$matches` collection through PowerShell's automatic `$Matches` variable. Renaming the collection to `$projectOpeners` fixed exact-card selection; the same ValuePattern/InvokePattern flow passed create, write/readback, Library return, and reopen retention against a fresh synthetic development project. This does not qualify an installed package. Run 33990078341 predates that correction and is superseded.

Run [33990404236](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990404236) repeated the installed-release flow from `1e6e556f5c63eacb9cf8a5685d4b6933eb5db09b` with the corrected harness. Install, release identity, synthetic creation, and editor text readback passed, but the flow again timed out waiting for Library after **All projects**. The owned-window diagnostic showed the editor retained its text while waiting in **Checking saved version…** with a **save response was lost** warning. The confirmed cause was a production command registration defect: `validate_snapshot` and its handler entry were compiled only under `debug_assertions`, although the release save path invokes that shared validator. This was a release persistence bug, not an unresolved UIAutomation finder failure.

After removing those debug gates while retaining debug-only trial, test-data, and CDP gates, the pinned `build` wrapper completed successfully and produced `target/release/webnovel-desktop.exe` (16,075,264 bytes; SHA-256 `e5d2edd96020d4d01cd0ed749373b56fa6df5c57123f04ca911e0e5babea144d`). The same owned UIAutomation flow, run against that exact release executable with a fresh synthetic `LOCALAPPDATA`, passed create, ValuePattern manuscript readback, saved status, Library return, project reopen, and persisted text readback. This is a local release-executable verification; it does not replace a fresh installed-package lifecycle run.

The fresh package [run 33991472684](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33991472684) for `508aee194fafa44a76afcd2885d42559d9d77853` built installer SHA-256 `0f047d56f9fe81660f1f81ba6fe5c49dac1eb74b0d7bfef9a13fae980fd7e073`. Installation, writing, exact text retention after reopening, and normal root-process close passed. Uninstall returned 0 after 1.6 seconds, but the immediate executable-absence check failed, so reinstall was not attempted. No forced process stop was used. This confirms the release save correction and remains a failed full lifecycle run.

The harness had waited for the uninstaller bootstrap process. NSIS documents that the usual uninstaller copies itself to a temporary directory; its final `_?=` argument disables that copy so a caller can wait for removal to finish. The generated Tauri installer uses this same argument when waiting for an older uninstall. The harness now supplies `_?=` with the validated synthetic installation root and retains both the successful-exit and executable-absence checks. It does not delete application data or erase a failed uninstall's remaining files. This is a supported lifecycle-wait correction; the next hosted run must prove it. In-place uninstall can retain its own executable and is not a test of the default self-copy cleanup. See the [NSIS command-line reference](https://nsis.sourceforge.io/Docs/Chapter3.html#3.2.2).

Independently, development [run 33991463598](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33991463598) for `508aee1` passed Windows/Ubuntu contracts and all 26 strict native checks on WebView2 `151.0.4129.101`, including approved writing briefs, clipboard, exact chosen-path export, and focus restoration.

## Evidence to record

The tracked `windows-package-smoke.yml` workflow and `scripts/windows-package-qualification.ps1` provide an opt-in hosted-runner lifecycle. They require a fresh runner-owned directory and an absent release data root, build the pinned package, record source/lock/installer identities, then exercise synthetic write/reopen, normal root-process close, default uninstall, and same-version reinstall retention. Partial results and forced cleanup cannot pass. Package lifecycle run `33987543086` installed with exit 0, opened Library, created a synthetic project/document, and entered/read back text, then timed out locating the project after returning to Library. The later 33990404236 investigation confirmed the release-only `validate_snapshot` registration defect described above; no installed-release pass is claimed for either historical artifact. This narrow smoke does not qualify an offline machine without WebView2, a true version upgrade, or descendant cleanup after normal root exit.

The installed release hides the session-only editor trial. The shared `validate_snapshot` IPC command is present in release builds because production save validation depends on it; only the trial UI/runtime, synthetic data/WebView, and CDP overrides remain debug-only. The package smoke checks the release Library for absence of the trial action. No installed-release pass is recorded until a fresh package run completes the full lifecycle.

Record the exact source commit, Cargo/npm locks, installer SHA-256, bundled WebView2 installer identity, OS/WebView versions, and installation target. Verify the installer and its included runtime separately from the debug CDP harness.

The remaining package trial must exercise:

1. Offline install and launch on the nominated Windows configuration, including a machine without the required WebView2 runtime.
2. Create/write two projects, normal close, reopen, and an upgrade that retains the same library and project data.
3. Native backup/recovered-copy/export dialogs, cancellations, existing destinations, and chosen-file content.
4. Keyboard/dead-key input, clipboard and external formatted paste, focus, accessible names and screen-reader use, physical minimum-window resizing, high DPI, and long chapters.
5. Process/renderer interruption, recovered terminal history, buffer retention under save failure, and cleanup of application-owned helpers.
6. Uninstall/reinstall behavior without unintended author-project deletion. Document any explicit remove-data option and keep it opt-in.

An unsigned development installer, successful local build, or emulated viewport does not by itself close these gates. See W7 in the full implementation ledger.
