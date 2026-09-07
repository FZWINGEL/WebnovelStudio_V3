# WebnovelStudio V3.0.0 Release Preparation

Status: **private Windows development candidate prepared; unreleased**.

This document is the preparation checklist for the current 3.0.0 candidate. It does not announce a public release and does not claim that the complete V3 roadmap is finished.

## Candidate scope

The candidate is a native Tauri desktop application for English web novels. Wuxia, xianxia, cultivation, progression, and translated-register writing are supported styles within that English authoring scope.

The implemented development surface includes:

- a per-project tabbed workspace for chapters, worldbuilding, characters, plot and themes, and notes;
- AI-first drafting, continuation, development, persistent discussion, scoped feedback, context inspection, and explicit review before changes are applied;
- the model picker and traits controls, dynamic Codex discovery, Claude author integration, and configurable OpenAI-compatible endpoints;
- GPT-5.6 Luna with xhigh reasoning for summaries and Story Memory maintenance;
- source-linked story context, reviewed knowledge and promise history, bounded lookup, local SQLite persistence, backup/recovered-project handling, import, and export.

Claude live behavior, hosted HTTP behavior, and broad provider coverage remain qualification boundaries. The primary **Create with AI** workflow redesign remains deferred while development tooling is optimized. The candidate does not claim literary-quality validation or that every planned V3 feature is complete.

## Identity and data boundaries

The version source is the Cargo workspace package version, currently `3.0.0`. The core and desktop Cargo packages inherit it. The frontend package and lock metadata match it. The Tauri configuration intentionally omits its duplicate version and inherits the desktop Cargo package version.

The stable application identifier remains `com.webnovelstudio.v3`. A packaged build stores release data under `%LOCALAPPDATA%\com.webnovelstudio.v3`. Debug builds use a checkout-specific development location and keep trial WebView/CDP overrides in the debug path.

## Verification commands

Run the version checks from the repository root:

```powershell
node scripts/check-versions.mjs
node --test scripts/check-versions.test.mjs
.\scripts\desktop.ps1 -Command check
```

The Windows packaging command is:

```powershell
.\scripts\desktop.ps1 -Command package
```

The package command is the source of the NSIS installer evidence. The verified candidate is:

| Evidence | Value |
| --- | --- |
| Build source | `63770b9122f598b3e32ea1b0f5f4020c4325115f` |
| Standard CI | [34067931031](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067931031), both jobs passed |
| Installer and lifecycle | [34067936098](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067936098), passed |
| Installer | `WebnovelStudio V3_3.0.0_x64-setup.exe` |
| Installer SHA-256 | `eef43925590d588f44e2e35978597e8aabc0b323f081c97f5092c45effc0fff2` |

The installed release passed synthetic project/chapter creation, writing, save/reopen, normal close, in-place uninstall, and same-version reinstall with project, document, and exact text retained. The installed ProductVersion was `3.0.0`; there were no errors or forced process stops. Evidence is retained under `.local/ci-34067936098`.

The downloaded installer is retained locally at `.local/builds/3.0.0/WebnovelStudio V3_3.0.0_x64-setup.exe` with `artifact.json` and `build-metadata.json`. Its 269,146,426 bytes, SHA-256, ProductName, and ProductVersion were verified against the hosted artifact. Previous delivered builds remain available.

[Retest 34068729080](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34068729080) passed the same lifecycle using that exact installer and the later documentation-only qualification source `ded8f3d43e0a14589cfd6afbde866f39e61530ba`. Its metadata preserves the original installer build separately from the current harness source. It skipped dependency setup and compilation, finishing in 1 minute 59 seconds versus the fresh job's 16 minutes 20 seconds. Evidence is retained under `.local/ci-34068729080`.

The earlier run `34066312408` on `729d6bd` built a 3.0.0 installer but failed before project creation because the harness searched for `Library` instead of the current `Your library` label. Its metadata and failure evidence remain under `.local/ci-34066312408`; its installer was not retained by the old success-only upload step. It is superseded by the passing candidate above.

## Private candidate preparation

- [x] Synchronize Rust, Tauri, npm, and lock-file versions through the workspace version guard.
- [x] Preserve the stable application identifier and separate release/debug data locations.
- [x] Pass hosted development CI run `34064355153` on `db3df283`: all 52 native checks plus the HTTP, close, interruption, recovery, and memory jobs.
- [x] Pass the local 3.0.0 tooling check: 721 Rust tests, 434 frontend tests, 11 tooling checks, formatting, strict Clippy, TypeScript, and the production build in 39.41 seconds.
- [x] Build and retain the 3.0.0 installer and its metadata.
- [x] Confirm hosted checks for candidate source `63770b9`; subsequent documentation changes do not alter application code.
- [x] Complete installed package lifecycle qualification: launch, project creation, chapter creation, close/reopen, and same-version uninstall/reinstall retention.
- [x] Qualify reuse of the exact installer after an allowed documentation-only change without rebuilding it.

## Broader distribution and author qualification

These gates are separate from private candidate preparation. Public distribution scope remains undecided, and none of the following is implied by the private candidate status:

- [ ] Qualify offline installation and launch, including the no-WebView2 case.
- [ ] Qualify a true upgrade while retaining library and project data.
- [ ] Qualify native backup, recovery, export, keyboard and paste behavior, accessible names, high-DPI layout, minimum window size, and long chapters.
- [ ] Qualify interruption, save failure, helper cleanup, uninstall/reinstall data behavior, and the live provider paths covered by the package checklist.

Use [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) for the implementation boundary and open V3 work. Use [WINDOWS_PACKAGE_QUALIFICATION.md](WINDOWS_PACKAGE_QUALIFICATION.md) for the detailed package and native trial procedure. Use [TESTING.md](TESTING.md) for the local verification matrix and [the README](../README.md) for the current user-facing setup.

Private candidate preparation is complete only when its version, verification, hosted, installer, and installed-lifecycle items have current evidence. Broader distribution and author qualification remain separate. This record must not be read as a claim that all roadmap work, live provider behavior, migration coverage, or public release decisions are complete.
