# Story Workshop completion checkpoint

This is a compact completion record for the current bounded Workshop checkpoint.
It supplements the detailed [implementation ledger](V3_STORY_WORKSHOP_IMPLEMENTATION.md)
and does not replace the [UX specification](V3_STORY_WORKSHOP_UX_SPEC.md).

## Status and source boundary

Workshop product source is `da08d8c62b7dc134119440749475d1373868caf5`.
The three planned development slices have functional implementations. The final
local check passed; full specification acceptance still requires native/provider
qualification and actual author evaluation. This is not a release-readiness or
narrative-quality claim.

## Implemented contract

- Story possibilities are separate `unresolvedQuestion`, `intendedPayoff`, and
  `possibleArc` records. Authors can open or archive records, edit them directly
  including to empty, and explicitly Keep, prefill, or Prepare them. The surface
  enforces a 64-record cap.
- Active nonempty possibilities are actor-frozen typed context, not canon.
  Planned context, actual delivered context, and the recap are visible so the
  author can inspect what is prepared and what was used.
- Themes provide optional Warmth, Hope, Unease, and Wonder controls, with a
  separate restrained/explicit injury-intensity control. Values remain editable
  and neutral until saved; the Rust preference family is carried into the prompt.
- Lens-aware next-question selection avoids current or saved dispositions and
  exact questions already present in working or chosen text. This is a bounded
  selection rule; it makes no claim of semantic understanding.
- Candidate detail picks are stale-review gated. Multi-target adoption retains
  the existing reviewed, atomic boundary; current core changes also reject
  dangling relationship references and verify the stored preview request
  fingerprint and target binding.
- Character development does not require a dedicated structured spine form.
  The optional lens and working text support development, while selected-details
  synthesis and atomic reviewed multi-target adoption provide the batch path.
  There is no unsolicited all-candidates adoption.
- The context panel uses a native HTML dialog with visible Tab/Shift+Tab
  behavior, background blocking, Escape restoration of the opener, and the
  desktop panel alongside it.

## Evidence recorded for this checkpoint

- The actual headless Chromium fixture at 1440 and 800 pixels passed typed
  edit/clear/archive/reopen/reload, explicit theme save, and the boundary with
  no start, adoption, or manuscript calls. It reported no overflow or page
  errors; screenshots were inspected. Evidence: `.local/workshop-completion-qa/report.json`.
- The 800px checks covered 40 Tab steps, background blocking, Escape focus
  restoration, and resize.
- The new context panel has four focused tests, and TypeScript passes.
- The full local wrapper passed **774 Rust tests** (73 core unit, 628 integration,
  73 desktop; one intentional ignore), **594 frontend tests in 56 files**, and
  **20 tooling checks**, plus formatting, strict Clippy, TypeScript, and production
  build. Log: `.local/workshop-completion-check.log`. These totals describe the
  combined checkout, including concurrent test/CI improvements that are excluded
  from the Workshop commit.
- An isolated archive of the Workshop commit, with its committed fixtures, also
  passed TypeScript and all 45 Workshop shell tests. This verifies the commit
  without depending on the concurrent extraction of request-building code.
- Rust schema 37 preserves old immutable state and packet bytes. SQLite tests
  cover save/reopen, empty rows, excluded archived/blank context, exact historical
  replay, request budgets, unchanged chapters, relationship references, and
  adoption-request integrity.

## Desktop build

`desktop.ps1 -Command spike` rebuilt
`D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe` without launching it.
It is 48,827,904 bytes, ProductVersion `3.0.0`, modified
`2026-09-07T21:41:16.3227704Z`, SHA-256
`1d1e4657a36b57a7ecf26ffdfc328367b4309860ba58a24b4d5322cacdfcfb79`.
The build used the combined checkout, including a concurrent request-helper
extraction; it is not a clean-source installer candidate. Its log and identity
are `.local/workshop-completion-debug-build.log` and
`.local/workshop-completion-debug-build.json`. The pre-existing large-bundle
warning remains. No native app or desktop keyboard automation was launched.

## Remaining gates

Native WebView2 execution, fresh live-provider coverage, current installer
qualification, and the author study remain open. W44 has no author observations.
The latest hosted CI attempt, [34160853258](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34160853258),
was blocked before test steps by the reported billing/spending-limit condition.
Passing local checks does not close those gates. The author-study facilitator
kit is ready; author observations must come from actual participants.
