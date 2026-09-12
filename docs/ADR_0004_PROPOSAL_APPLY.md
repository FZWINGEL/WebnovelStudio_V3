# ADR 0004 — Proposal review and single-author Apply

**Date:** 5 September 2026
**Status:** accepted local W5 checkpoint; development CI green, broader installed-release/native qualification pending

This ADR defines the bounded proposal/review contract that extends [ADR 0001](ADR_0001_EDITOR_CONTRACT.md), [ADR 0002](ADR_0002_AUTHOR_GUIDANCE.md), and [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md). Discussion and proposal preparation never mutate the manuscript or establish canon.

## Decision

### Intent and context

- `Discuss` remains the wire-compatible default. `ProposeEdits` is an explicit feedback intent.
- `Discuss` uses the author-room context. `ProposeEdits` currently requires an explicit selected passage in a chapter and uses `Working` plus `RestrictedWriting`/`Revise` at the target chapter's reader frontier.
- Restricted proposal packets exclude author-room private material, future material, current guidance, and recent chat. Whole-chapter proposals are outside this slice.
- Retry and durable composer state retain the intent; legacy payloads omit `Discuss` on serialization and default to `Discuss` on deserialization, preserving existing payload hashes.

### Provider terminal and review records

- The provider terminal accepts only a strict `ProposalOutput` containing one to three `ProposalCandidate` values. Malformed, unsupported, or over-limit output is retained as raw, unplaced discussion text; no repair call is made. The deterministic mock produces three alternatives.
- Candidates, prepared versions, and decisions are immutable. Preparing a historical proposal is allowed against its immutable original source; the exact JavaScript result, text, marks, and block scope must validate against that source with compare-and-swap versions. Only Apply requires the current document head, source epoch, and policy. No manuscript mutation occurs during preparation.

### Apply and Reject

- Apply is an explicit author action over the exact prepared replacement. One transaction validates the current head, source epoch, policy, proposal, prepared version, and receipt identity, then records the body, incremented source epoch, before/after revisions, decision, and operation receipt.
- Any source edit or current-policy change makes a pending proposal stale. Stale proposals remain available for inspection but cannot apply. Reject records an explicit decision and receipt without changing the manuscript or source epoch.
- Receipt identity fences cross-operation collisions. A repeated Apply operation returns the separate latest result and latest document head with `already_applied`; it does not apply again. Copied historical proposals, snapshots, and receipts are read-only and cannot authorize writes.

### Editor handoff

The parent editor session places a pending-Apply barrier before flush and preflight, commits the durable operation first, then dispatches the exact existing-editor transaction with `closeHistory`. Newer edits are retained on conflict. Lost acknowledgments enter an uncertain fence and reconcile by operation identity; this path does not autosave.

## Current bounds and qualification

This W5 checkpoint has 10 core proposal tests plus one real-process-kill Apply test, one desktop mock test, nine frontend Apply tests plus two ProseMirror history tests, and native IPC mock commands. These focused tests pass. The prior local wrapper evidence passed 168 active Rust tests (167 core plus one desktop test) and 122 frontend tests. A diagnostic Tauri/WebView2 W5 subset passed 20/20 after waiting for the `Your stories` heading before reload, including preview editing, navigation/reload retention, exact Apply with protected ending, stale pending proposals, Reject without change, and undo/redo body and decision retention after reload; the proposal preview/applied captures were visually inspected and readable. The current strict full native22 flow still fails locally at the existing W0 clipboard Ctrl+V step in current and old binaries because paste receives an empty `DataTransfer`; a 100 ms yield did not fix it, and no skip was added. The observed clipboard cause remains unresolved; a diagnostic `Get-Clipboard -Raw` query also failed, and no competing clipboard holder was found. The ignored `.local/native-other-results` output omits clipboard coverage. The pushed checkpoint [`a2a0163`](https://github.com/FZWINGEL/WebnovelStudio_V3/commit/a2a01632890a44efbe84bc526674fc9bf06d3d94) is covered by [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728), green across Ubuntu/Windows contract jobs and Windows native strict21. This is development evidence; it does not qualify an installed release or live provider.

Structured complete-block and whole-chapter suggestions now extend this protocol under [ADR 0020](ADR_0020_STRUCTURED_SUGGESTIONS.md), while the legacy Passage path remains unchanged. Batch Apply, arbitrary partial multi-block editing, and manual rebind remain open. [Codex qualification](CODEX_QUALIFICATION.md) records native discovery and bounded requests, including one structured suggestion, but production provider support remains unqualified. The current W6 development slice adds saved-version comparison and explicit document restore under [ADR 0005](ADR_0005_DOCUMENT_HISTORY.md); final W6 qualification remains pending. W5 does not close the B trial, C4–C6, or the broader V3 qualification gates.
