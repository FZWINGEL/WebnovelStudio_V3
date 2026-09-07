# Windows package qualification

## Current Workshop completion debug build — 7 September 2026

Workshop product source is `da08d8c62b7dc134119440749475d1373868caf5`.
The combined checkout passed 774 Rust / 594 frontend / 20 tooling checks,
formatting, strict Clippy, TypeScript, and production build. Concurrent test/CI
work and a request-helper extraction remain outside the Workshop commit; its
isolated frontend archive passed TypeScript and 45 Workshop shell tests.

`desktop.ps1 -Command spike` rebuilt
`D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe` without launching it.
Identity: 48,827,904 bytes, ProductVersion `3.0.0`, SHA-256
`1d1e4657a36b57a7ecf26ffdfc328367b4309860ba58a24b4d5322cacdfcfb79`,
modified `2026-09-07T21:41:16.3227704Z`. The log and JSON identity are
`.local/workshop-completion-debug-build.log` and
`.local/workshop-completion-debug-build.json`. This combined-checkout debug build
is not a clean-source installer candidate and does not qualify native runtime
or author experience. See the [completion record](V3_STORY_WORKSHOP_COMPLETION.md).

Hosted [CI 34164413029](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34164413029)
for pushed head `074fc96` failed before any step in `contracts` or
`windows-native`. Both annotations report an account payment/spending-limit
problem. `.local/workshop-completion-hosted*.json` retains the exact job and
annotation evidence. No native runtime result or installer was produced, and no
manual rerun or package dispatch was made.

## Preceding notes-organization checkpoint

The preceding notes-organization source is
`d3939aed349cfa4aa811c919fa6ec3cc89f10513`. Current [CI
34160853258](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34160853258)
was refused before steps in both jobs because GitHub reported recent account
payment failure or a spending-limit requirement; no hosted test steps ran. Its
job and annotation evidence is retained under `.local/workshop-notes-hosted/`.
No manual retry or package dispatch was made. No current installer or package
qualification is claimed.

## Preceding notes-organization debug build — 7 September 2026

`desktop.ps1 -Command spike` rebuilt the current source without launching it.
The executable is `D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe`,
48,696,832 bytes, ProductVersion `3.0.0`, SHA-256
`09b26869de158b7b804c48c133f58724072fb1cb2231d8119bb8173860581fe5`,
modified at `2026-09-07T20:46:31.1567787Z`. The build log and identity are
`.local/workshop-notes-debug-build.log` and
`.local/workshop-notes-debug-build.json`. The final local wrapper passes 771
Rust / 566 frontend / 11 tooling checks, plus formatting, strict Clippy,
TypeScript, and production build. This debug executable was not launched. The
native W06 harness is syntax-checked and statically reviewed only. These facts
do not qualify native runtime behavior or an installed package.

The preceding recap/sample/context source is
`7eb94fbff3380688786c54a78e2e3dcdaea815bc`. Fresh
[CI 34156441884](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34156441884)
and [package run 34156451569](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34156451569)
failed at job admission with no steps executed. GitHub reports an account
billing or spending-limit issue. No installer was built by that package run;
the preceding successful installer below does not qualify these changes.
Exact run/annotation evidence is in `.local/workshop-context-samples-hosted/`.

The initial distribution target is Windows x64 with an NSIS installer. Package configuration is implementation work; a successful build alone does not establish offline installation, accessibility, recovery, or author-trial acceptance. Current executed evidence is recorded in [implementation status](IMPLEMENTATION_STATUS.md).

## Preceding candidate-alternatives checkpoint — 7 September 2026

The candidate-alternatives refinement was built locally with
`desktop.ps1 -Command spike` at `2026-09-07T18:54:27.2716088Z`. The retained
local executable identity is
`D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe`, 48,686,592 bytes,
ProductVersion `3.0.0`, SHA-256
`e2f73f3f1b66a360deac4f7add25ceace9942bb48f9f4041768fe3bfb76b3d8f`.
It was not launched locally. The build log and identity are
`.local/workshop-alternatives-debug-build.log` and
`.local/workshop-alternatives-debug-build.json`. It is an earlier local build
identity; hosted qualification below covers the exact committed source.
The preceding committed product source is
`16540ea9f968e969f63b57817f36c2a187894516`.
[CI 34153931657](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34153931657)
is terminal after 19 Workshop groups passed and the aliases immediate-read
failure at line 676; [package run
34153949693](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34153949693)
passed its bounded installed lifecycle on that preceding source.

The native failure read the aliases textbox immediately at mount, before waiting
for alias-load readiness: expected `Ash Wren\nLin Qiao\n林乔`, actual empty.
Earlier save/DB/reopen checks passed, so this indicates a harness timing race;
it provides no alias-loss evidence. `pageErrors=[]`, and the evidence under
`.local/ci-workshop-34153931657/native-spike-evidence/workshop/` contains no
alias-loss observation. This does not establish a broad native pass or qualify
the current recap/sample/context slice.

## Preceding recap/sample/context debug build — 7 September 2026

That checkpoint's debug executable was rebuilt after the protected-text correction
from the uncommitted recap/sample/context sources. It is
`D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe`, 48,690,688 bytes,
ProductVersion `3.0.0`, SHA-256
`360e4e9d1e2614dc1cd2887b714ebb1785d8e65148f25b917bded40e0bc86abe`.
It was not launched locally. The build log and identity are retained in
`.local/workshop-context-samples-debug-build.log` and
`.local/workshop-context-samples-debug-build.json`. This debug identity has no
hosted native or package qualification yet. Its source is committed at
`7eb94fb`; the fresh runs above were refused at job admission.

## Preceding checkpoint installed package — 7 September 2026

Package run [34153949693](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34153949693)
passed on the clean preceding source
`16540ea9f968e969f63b57817f36c2a187894516`. ProductVersion
is `3.0.0`; installer SHA-256 is
`f0a1ca379a1239d89c92fcc0b0155ba613e7c96f90ccb5187700093ee32057f0`.
The hosted Windows Server 2025 runner installed the release, created and wrote
a synthetic project/document, read the text after reopen, and closed normally.
Default uninstall and same-version reinstall retained the project, document,
and text. Installer and uninstall exit codes were 0, `errors=[]`, and no forced
process stop occurred. The result records `upgradeQualification=false`.
Build metadata and lifecycle evidence are retained under
`.local/package-workshop-34153949693/`. This is bounded installed lifecycle
evidence, not qualification of the current recap/sample/context
slice, Workshop-native acceptance, offline/no-runtime installation, a true
upgrade, live-provider behavior, or author-study qualification. A fresh package is required once hosted job admission is available again.

## Previous controls development build

The Workshop controls source `fc3468829e037588458a05204c0ef92ddbea9cc2`
built successfully with `desktop.ps1 -Command spike` at
`2026-09-07T18:15:05.5439967Z`. The executable is
`D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe`, 48,686,592 bytes,
ProductVersion `3.0.0`, SHA-256
`10de871c7e2fb7d90ba2f4ee7c40c995c74a19678661422248c2a22534828d1e`.
The log and identity are retained in `.local/workshop-controls-debug-build.log`
and `.local/workshop-controls-debug-build.json`. It was not launched locally.
[CI 34150860150](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150860150)
passed its contracts and 17 Workshop groups, then failed on a character-row
harness selector. Later native suites were skipped. Both row selectors are
corrected, and native requalification in [CI
34152622887](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34152622887)
on harness source `c7c1c6a` reached 25 groups before a what-if fixture prototype
mismatch. The strict document comparisons now normalize SQLite row shapes;
later native suites were skipped in that run. Fresh
[package run 34150884037](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150884037)
passed the lifecycle recorded below; no older installer was reused.

## Latest qualified Workshop package — 7 September 2026

[Package run 34150884037](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150884037)
passed on clean builder and qualification source
`fc3468829e037588458a05204c0ef92ddbea9cc2`. ProductVersion is `3.0.0`; installer
SHA-256 is `4c506a51da20a189a31d7d738786401024457307c637bb188acdeea49443aec6`.
It completed at `2026-09-07T18:32:13.4292995Z` with no errors. UIAutomation
verified synthetic project/document creation, text entry/readback, reopen, and
retained project/document/text after the identical installer was uninstalled
and reinstalled. Normal close succeeded without forced process stop.

Metadata and lifecycle evidence are retained under
`.local/package-workshop-34150884037/`. The installer remains the hosted
`windows-installer` artifact (10029642333), not a local download. This is bounded
installed lifecycle evidence, not full Workshop, true upgrade, offline runtime,
live-provider, or author-study qualification. It predates the latest
candidate-alternatives refinement above.

## Previous names/aliases development build

The names/aliases source built successfully with `desktop.ps1 -Command spike`
at `2026-09-07T17:24:48.8272045Z`. The executable is
`D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe`, 48,684,032 bytes,
ProductVersion `3.0.0`, SHA-256
`f171aa81b37e908b96b7d10d13b47d77054040fa35adad4dad67418a1022512e`.
The build log and byte identity are retained in
`.local/workshop-aliases-debug-build.log` and
`.local/workshop-aliases-debug-build.json`. This build was not launched locally;
its native and installer qualification requires the fresh hosted runs.
Source `b801ae9cfc00203a37c7de05da8de801a0214af1` was checked by
[CI 34147701184](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147701184)
and fresh [installer run 34147720364](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147720364).
CI is terminal: contract steps passed, then the Workshop harness failed after
16 groups because the tab selector omitted the visible character count. That
selector passed in CI 34150860150. The installer job passed the bounded
installed lifecycle below; it does not exercise the aliases controls.

## Previous names/aliases package — 7 September 2026

Fresh [package run 34147720364](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147720364)
passed on clean builder and qualification source
`b801ae9cfc00203a37c7de05da8de801a0214af1`. ProductVersion is `3.0.0`, SHA-256
`e932b1c02f8f93e9df7c66cd069a3f09a58b0c23d97d2cb78a4057eb6376bb6a`.
The evidence records synthetic project/document creation, text entry/readback,
reopen, successful normal close without forced process stop, and retained
project/document/text after same-version uninstall/reinstall. It completed at
`2026-09-07T17:45:34.0829893Z` with no errors. It does not qualify true upgrades,
offline runtime installation, live providers, author data, or the later product
corrections. Metadata and lifecycle evidence are retained under
`.local/package-workshop-34147720364/`; the installer remains the hosted
`windows-installer` artifact and was not downloaded locally.

## Previous Workshop package

Fresh [package run 34145254658](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34145254658)
passed on clean builder and qualification source
`e61640a4738128b9744919275e362e402c7ed0d8`. ProductVersion is `3.0.0` and the
installer SHA-256 is
`59394f6903ff4cc633e47556cffc3e921a7784cfa32a3da4bd68fcfcaf3ea495`.
The synthetic installed lifecycle completed at `2026-09-07T17:10:32.2696907Z`:
project/document creation, text entry/readback, normal close/reopen, and
same-version uninstall/reinstall retained the project, document, and text.
The result reports `errors=[]`, no forced process stop,
`sameVersionReinstallOnly=true`, and `upgradeQualification=false`. It does not
qualify upgrades, offline/no-runtime installation, live providers, or author
usability.

Downloaded metadata, results, and screenshots are retained under
`.local/package-workshop-34145254658/`. The installer itself remains in this
run's `windows-installer` GitHub artifact; it has not been downloaded locally.
This is a previous product checkpoint and does not represent the current alias
UI/IPC tree; no native alias qualification is claimed. Separately, native CI
[34145255173](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34145255173)
passed its contracts and 21 Workshop checks, then stopped when the still-open
working-story sidebar intercepted the W30 what-if control. That run is not a
complete pass; the retained failure evidence is under
`.local/ci-workshop-34145255173/workshop/`, and the harness now closes the
sidebar through its visible in-sidebar control before proceeding.

The immediately preceding package run [34142007124](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34142007124)
remains retained under `.local/package-workshop-34142007124/` with installer
SHA-256 `954416c66c8982f03113b643550dda6d39539539cfaf3a608be4911f13d1cc01`.

## Earlier locally retained Workshop package — 7 September 2026

The subsequent preset/Story Bible hardening changes are not in this installer.
They require a fresh builder and installed-lifecycle run; this retained artifact
continues to identify the earlier source below.

Package run [34130743953](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34130743953)
succeeded against the exact clean builder and qualification source
`df747b6d4f8d9496cb00f1fc5f0b53285d4b5edf`, completing at
`2026-09-07T14:20:03.2244053Z` on Windows Server 2025 `10.0.26100` with
WebView2 `151.0.4129.101`. ProductVersion was exactly `3.0.0`. The installer is
`.local/builds/3.0.0-workshop-df747b6/WebnovelStudio V3_3.0.0_x64-setup.exe`,
269,716,613 bytes, SHA-256
`8ea09a47e8e4a5408a46cf1645b60330e3314234e3256f5baa6dd1d3d97a85ef`, matching
the recorded metadata and results.

The installed lifecycle passed synthetic project/chapter creation, English text
save/reopen, normal close, in-place uninstall without delete-data, and
identical-version reinstall with project/document/text retained. The result has
`errors=[]` and `forcedProcessStop=false`. Evidence is in
`.local/ci-package-34130743953/build-metadata.json` and
`.local/ci-package-34130743953/run-20260907-141852-540/result.json`; the
same-version reinstall screenshot was visually inspected. This qualifies the
installed lifecycle only: offline/no-runtime installation, true upgrade,
live-provider behavior, author data, and broader native/quality
qualification remain open.

The historical debug executable from source
`df747b6d4f8d9496cb00f1fc5f0b53285d4b5edf` is
`target/debug/webnovel-desktop.exe`, 48,414,720 bytes, built at
`2026-09-07T14:02:46.9371672Z`, with SHA-256
`ad0210a80bf8e83045906ae4d36d30662af96fcf076663f90e27a373cc830c85`.
The native qualification checkout
`47a50d9f230fe06c3f46eba700fbac57b03bff11` differs only in Workshop native
harness and documentation; [CI 34133645198](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34133645198)
passed all 15 Workshop checks and the existing editor, HTTP, close,
interruption, recovery, and memory-lookup suites. Workshop used the local mock
and synthetic projects; its report has no page errors. Retained native
harness-failure histories and broader evidence
are recorded in the [implementation status](IMPLEMENTATION_STATUS.md) and
[Story Workshop implementation ledger](V3_STORY_WORKSHOP_IMPLEMENTATION.md).

## Historical 3.0.0 private candidate installer

This is private, unreleased historical evidence for the earlier candidate. [Package run 34067936098](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067936098) passed on clean builder source `63770b9122f598b3e32ea1b0f5f4020c4325115f`. The installer `WebnovelStudio V3_3.0.0_x64-setup.exe` has SHA-256 `eef43925590d588f44e2e35978597e8aabc0b323f081c97f5092c45effc0fff2`. Windows Server 2025 `10.0.26100` with WebView2 `151.0.4129.101` passed installed release identity, exact ProductVersion `3.0.0`, synthetic project/chapter creation, writing, save/reopen, normal close, in-place uninstall, and same-version reinstall with project/document/text retained. There were no errors or forced process stops. Downloaded metadata, results, and screenshots are under `.local/ci-34067936098`; the reinstall screenshot was visually inspected.

[Retest 34068729080](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34068729080) passed the same lifecycle with the exact original installer and qualification checkout `ded8f3d43e0a14589cfd6afbde866f39e61530ba`, which changes documentation only. Rust setup, dependency installation, and compilation were skipped. The fresh job took 16 minutes 20 seconds; the retest took 1 minute 59 seconds. Its evidence under `.local/ci-34068729080` retains `installerBuild` source `63770b9` separately from `qualificationSource` `ded8f3d` and the harness hash. See [testing](TESTING.md) for the allowed-change and identity checks. Offline/no-runtime installation, true upgrades, installed live-provider behavior and broader author qualification remain open.

The preceding package attempt used builder source `729d6bdfe3f657badac80b114aee0f8a0b6b2970` in run `34066312408`. It built the 3.0.0 installer, then stopped before project creation because the qualification harness waited for the stale UIAutomation label `Library` while the current UI exposes `Your library`. That run remains failure evidence; it is not package qualification.

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

## Historical 0.1.0 installed-lifecycle checkpoint, 6 September 2026

Package [run 34020898065](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34020898065) qualified source `12b8dd0c2ce50224de844ab8a557f912708d1bfc` on Windows Server 2025 `10.0.26100` x64 with WebView2 `151.0.4129.101`. The installer `WebnovelStudio V3_0.1.0_x64-setup.exe` has SHA-256 `f4fb306a5d5775f0758cab0a78828b627afc2d9e40ce4b84a4237e9b9807d508`; the run completed successfully with no errors or forced process stop. It installed the release Library without the debug trial, created and saved synthetic English prose, reopened it with text retained, closed normally, removed application files through the in-place default uninstall without a delete-data option, and reinstalled the identical version with the project, document, and text retained. The run claims only installed-release, synthetic write/reopen, and same-version uninstall/reinstall retention. It excludes offline/no-runtime installation, a true upgrade, live-provider behavior, and author-data qualification. Evidence is retained in `.local/ci-34020898065/build-metadata.json` and `.local/ci-34020898065/run-20260906-081658-595/result.json`.

This package evidence is distinct from the later test/docs-only source `904c0ae`; that source does not establish package contents or installed-lifecycle behavior. The package source records Cargo lock SHA-256 `38044be23c9a941a54110fec37ba6cd7e88d5f519004fbd225a3589064a309a1`, npm lock SHA-256 `f0c20e859a7180b26019e2b6fa20876a3e43881d1921b73b1500f5b3cf21662c`, and Tauri config SHA-256 `a3e0c9e75a50263ca52a4c65ab6be626f2407f597b6f969243b699bb0b00135a`.

## Prior installed-lifecycle checkpoint, 6 September 2026

The same clean source `c92df7708a4a48b83c11f25866c1f0a39d4a94b7` also passed [package run 34017567399](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34017567399) on Windows Server 2025 `10.0.26100` with WebView2 `151.0.4129.101`. It installed, opened the release Library without the debug trial, created and saved synthetic English prose, reopened it, closed normally, uninstalled in place, and reinstalled the same version with project/document/text retention. The result has no errors and no forced process stop. Completion was `2026-09-06T07:01:49.6493638Z`; installer SHA-256 is `5fa3af0e572c40c9321b9da15ec766d4d823fd46d770316bd285a3d4718f6947`. Downloaded evidence is `.local/ci-34017567399/build-metadata.json` and `.local/ci-34017567399/run-20260906-070045-364/result.json`; the retained reinstall screenshot was visually inspected. This is narrow installed-lifecycle evidence, not offline/no-runtime installation, an upgrade, installed live generation/structured editing, or full release acceptance.

The clean source records Cargo lock SHA-256 `38044be23c9a941a54110fec37ba6cd7e88d5f519004fbd225a3589064a309a1`, npm lock SHA-256 `f0c20e859a7180b26019e2b6fa20876a3e43881d1921b73b1500f5b3cf21662c`, and Tauri config SHA-256 `a3e0c9e75a50263ca52a4c65ab6be626f2407f597b6f969243b699bb0b00135a`. The unsigned package is version `0.1.0`; its installed target was the fresh runner-owned `D:\a\_temp\webnovel-package-qualification\app`. The result retains historical field names containing `defaultUninstall`, but its event log proves the in-place `_?=` invocation. It does not qualify the default uninstaller self-copy cleanup.

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

Package [run 33992126374](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33992126374) installed the release successfully, confirmed the Library, and created the synthetic project, but the qualification harness timed out waiting for a UIAutomation `ListItem` named **Chapter** after expanding the document-kind selector. The failure screenshot showed the HTML selector already displaying **Chapter**, while the bounded UIAutomation snapshot exposed no option item. The harness now reads the combo's selected value through `SelectionPattern` or `ValuePattern`, accepts an already-selected **Chapter**, and only expands and chooses an option when the current value differs. The run remains a failed package qualification; this correction does not claim a new installed-release pass.

## Evidence to record

Live-provider/import source `568959740045bfbb9202c4370f4e3d26db360c11` passed [package run 33997290012](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33997290012), completed `2026-09-05T23:06:52.6074002Z`. The clean-source installer SHA-256 is `78be293c04bfcd0b174f9ca2a54f9e0dbc2fba1c6cccb17c91860d3a98ee3639`. Windows Server 2025 `10.0.26100` with WebView2 `151.0.4129.101` passed release Library, synthetic creation/writing/reopen, normal close, in-place uninstall, and same-version reinstall with retained text. There were no errors or forced process stops. This does not exercise live generation or V2 import through the installed package, and it predates author-review integration. The same source's [standard CI 33997192681](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33997192681) passed native28 but failed Ubuntu Clippy; that separate source correction needs a fresh cross-platform result.

The newer clean baseline `be1d93c579f50dcabf78b3151e4dd299b3c95d45` passed [package run 33994616334](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33994616334), completed `2026-09-05T22:10:16.1724126Z`. Its installer SHA-256 is `dec8ec5393c4453fd43a6f4ff36957f58eb32d45d08fe1f7f0068d4af02ce918`. Windows Server 2025 `10.0.26100` with WebView2 `151.0.4129.101` passed install, create/write/reopen, normal close, in-place uninstall, same-version reinstall, and retained text without forced stop or recorded errors. [Standard CI 33994609086](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33994609086) also passed both contract platforms and all 28 strict native checks. Both runs predate the live-provider/import changes. Offline/no-runtime installation, true upgrades, default self-copy uninstall cleanup, and broader release qualification remain open.

The first complete hosted lifecycle pass is [run 33993370498](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993370498), source `cd7fb77f0bac8bc2ce1756f28044d406c356ff20`, completed `2026-09-05T21:43:44.8017557Z`. Its installer SHA-256 is `9dcd7fc46dc8f259733d8487e11992b5c5b07d99dddd4a14f71c21baba8fd8ae`; Cargo lock `38044be23c9a941a54110fec37ba6cd7e88d5f519004fbd225a3589064a309a1`; npm lock `f0c20e859a7180b26019e2b6fa20876a3e43881d1921b73b1500f5b3cf21662c`. The clean Windows Server 2025 runner (`10.0.26100`, x64) had WebView2 `151.0.4129.101`. The downloaded build metadata, result JSON, and screenshots confirm install exit 0, release Library without the trial entry point, synthetic project/document creation, ValuePattern text entry/readback, saved text after reopen, normal close, in-place uninstall with retained data, and same-version reinstall with project/document/text retention. No forced process stop was used and the result has no errors. This qualifies that narrow installed-release lifecycle for this artifact. It predates persistent model settings and the subsequent process cleanup fix; it does not establish offline/no-runtime installation, a true version upgrade, external paste, screen-reader quality, live assistance, or full W7 acceptance.

The tracked `windows-package-smoke.yml` workflow and `scripts/windows-package-qualification.ps1` provide an opt-in hosted-runner lifecycle. They require a fresh runner-owned directory and an absent release data root, build the pinned package, record source/lock/installer identities, then exercise synthetic write/reopen, normal root-process close, in-place uninstall, and same-version reinstall retention. Partial results and forced cleanup cannot pass. Package lifecycle run `33987543086` installed with exit 0, opened Library, created a synthetic project/document, and entered/read back text, then timed out locating the project after returning to Library. The later 33990404236 investigation confirmed the release-only `validate_snapshot` registration defect described above; no installed-release pass is claimed for either historical artifact. This narrow smoke does not qualify an offline machine without WebView2, a true version upgrade, or descendant cleanup after normal root exit.

The installed release hides the session-only editor trial. The shared `validate_snapshot` IPC command is present in release builds because production save validation depends on it; only the trial UI/runtime, synthetic data/WebView, and CDP overrides remain debug-only. The package smoke checks the release Library for absence of the trial action. The narrow installed-release pass above does not close the broader package gates below.

The frontend now applies the editor-trial boundary at build time: the normal
production Vite build excludes the trial module and entry action, while the
Tauri hook's `TAURI_ENV_DEBUG=true` production-mode build retains the lazy
debug trial used by `test:native`. The final Tauri debug build and focused native
run open that lazy trial, display the sample, and return to the Library.
Evidence and executable identity are in [implementation status](IMPLEMENTATION_STATUS.md).
The separate release asset build excludes the trial/sample markers; a fresh
installed release remains a separate qualification gate.

Record the exact source commit, Cargo/npm locks, installer SHA-256, bundled WebView2 installer identity, OS/WebView versions, and installation target. Verify the installer and its included runtime separately from the debug CDP harness.

The remaining package trial must exercise:

1. Offline install and launch on the nominated Windows configuration, including a machine without the required WebView2 runtime.
2. Create/write two projects, normal close, reopen, and an upgrade that retains the same library and project data.
3. Native backup/recovered-copy/export dialogs, cancellations, existing destinations, and chosen-file content.
4. Keyboard/dead-key input, clipboard and external formatted paste, focus, accessible names and screen-reader use, physical minimum-window resizing, high DPI, and long chapters.
5. Process/renderer interruption, recovered terminal history, buffer retention under save failure, and cleanup of application-owned helpers.
6. Uninstall/reinstall behavior without unintended author-project deletion. Document any explicit remove-data option and keep it opt-in.

An unsigned development installer, successful local build, or emulated viewport does not by itself close these gates. See W7 in the full implementation ledger.
