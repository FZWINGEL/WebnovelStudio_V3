# WebnovelStudio V3.0.0 Release Preparation

Status: **private Windows development candidate; unreleased**.

This document is the preparation checklist for the current 3.0.0 candidate. It does not announce a public release and does not claim that the complete V3 roadmap is finished.

## Candidate scope

The candidate is a native Tauri desktop application for English web novels. Wuxia, xianxia, cultivation, progression, and translated-register writing are supported styles within that English authoring scope.

The implemented development surface includes:

- a per-project tabbed workspace for chapters, worldbuilding, characters, plot and themes, and notes;
- AI-first drafting, continuation, development, persistent discussion, scoped feedback, context inspection, and explicit review before changes are applied;
- the model picker and traits controls, dynamic Codex discovery, Claude author integration, and configurable OpenAI-compatible endpoints;
- GPT-5.6 Luna with xhigh reasoning for summaries and Story Memory maintenance;
- source-linked story context, reviewed knowledge and promise history, bounded lookup, local SQLite persistence, backup/recovered-project handling, import, and export.

Claude live behavior, hosted HTTP behavior, and broad provider coverage remain qualification boundaries. The candidate does not claim literary-quality validation or that every planned V3 feature is complete.

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

The package command is the source of the NSIS installer evidence. A fresh 3.0.0 package run has not yet been completed for this candidate.

## Preparation checklist

- [x] Synchronize Rust, Tauri, npm, and lock-file versions through the workspace version guard.
- [x] Preserve the stable application identifier and separate release/debug data locations.
- [x] Verify the test-optimization baseline: CI run `34064355153` passed on `db3df283`, before the 3.0.0 version change.
- [x] Pass the local 3.0.0 candidate check: 721 Rust tests, 434 frontend tests, six version-guard tests, formatting, strict Clippy, TypeScript, and the production build.
- [ ] Pass hosted checks on the actual 3.0.0 candidate commit.
- [ ] Run a fresh 3.0.0 package build and retain the installer, hashes, and package metadata as evidence.
- [ ] Qualify offline installation and launch, including the no-WebView2 case.
- [ ] Qualify close/reopen and a true upgrade while retaining library and project data.
- [ ] Qualify native backup, recovery, export, keyboard and paste behavior, accessible names, high-DPI layout, minimum window size, and long chapters.
- [ ] Qualify interruption, save failure, helper cleanup, uninstall/reinstall data behavior, and the live provider paths covered by the package checklist.

Use [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) for the implementation boundary and open V3 work. Use [WINDOWS_PACKAGE_QUALIFICATION.md](WINDOWS_PACKAGE_QUALIFICATION.md) for the detailed package and native trial procedure. Use [TESTING.md](TESTING.md) for the local verification matrix and [the README](../README.md) for the current user-facing setup.

This preparation record is complete only when the unchecked package and native gates have current evidence. It must not be read as a claim that all roadmap work, live provider behavior, migration coverage, or public release decisions are complete.
