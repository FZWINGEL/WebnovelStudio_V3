# Architecture ownership improvements — 12 September 2026

Implemented from the
[detailed design](../openspec/changes/strengthen-domain-ownership/design.md) and
[task ledger](../openspec/changes/strengthen-domain-ownership/tasks.md) in the
`arch-module` worktree. The starting head is `c91151b`; the comparison baseline
is `codex/v3-persistence` at `97d0fe0`. The earlier
[six review fixes](ARCH_MODULE_REVIEW.md) remain part of the local, uncommitted
worktree. This report covers the subsequent ownership increment.

## Changes

- Conversation owns the three run-selection queries Workshop consumes. A
  transaction-local candidate read context carries the existing connection and
  stateless reader, preserving query positions and raw-output validation policy.
- Document workspace and library own their state and operations separately.
  Library replacement/return uses the document owner's save and navigation
  barriers. The shell composes presentation, common operation admission and
  close; layout consumes explicit views and actions.
- Independent review found and repaired a late-navigation race during chat
  adoption. Navigation now checks the exact captured editor session, including
  a detached `null` session, before replacing a project or returning to the
  library. It cannot overwrite an editor restored while navigation was pending.
- Workshop result insertion and locking use store actions and read-only
  projections, preserving draft watermarks and uncertain save payloads.
- Binding generation rejects conflicting declarations and ambiguous filenames,
  selects one import owner for identical declarations, and checks the entire
  generated TypeScript inventory. Regeneration removes only obsolete marked
  outputs and refuses foreign files before writing.
- Source-aware Rust and frontend architectural checks protect the new ownership
  boundaries. The [current architecture map](ARCHITECTURE.md) replaces obsolete
  skeleton/layer claims while retaining the migration history.

## Focused evidence

| Check | Result |
| --- | --- |
| Conversation readers | 3 tests pass: ordering/filtering, exact operation owner, active transaction visibility/rollback |
| Workshop integration filter | 42 tests pass, including recovered candidates, linked adoption and stale atomic refusal |
| Lost Workshop receipt | Injected second-transaction failure replays the same committed run after state advances; changed budget is rejected |
| Architecture package | 9 tests pass, including Cargo graph fixtures and parsed source ownership fixtures |
| Binding integrity | 20 tests pass; strict package Clippy passes; all nine generated modules byte-identical across regeneration |
| Workshop store/consumers | 65 focused frontend tests and TypeScript pass, including watermark, notification, lock and prior flush-race coverage |
| Workspace ownership | 41 focused tests pass: 27 existing Workspace tests and 14 direct owner regressions |
| Adoption/navigation interleaving | Both deferred replacement/return regressions fail before the exact-session repair and pass afterward |
| Frontend boundary enforcement | 56 tests pass, covering the real import graph and positive/negative parsed-source fixtures |

The receipt regression creates a synthetic trigger that rejects only the
Workshop start receipt. It does not delete or rewrite immutable historical
receipts. The production path still has its existing discussion/start and
Workshop-receipt transactions; the test proves recovery across their boundary.

## Integrated evidence

The pinned `scripts/desktop.ps1 -Command check` completed successfully:

- 962 Rust tests passed; one intentionally ignored test remains.
- 794 frontend tests passed across 78 files; 25 tooling tests passed.
- Formatting, strict workspace Clippy, TypeScript and production build passed.
- Vite still reports its existing advisory about chunks above 500 kB. This
  increment does not claim a bundle-size or performance improvement.

Independent review passed after the adoption/navigation repair, with no
remaining material blocker in the assigned Rust, generator, boundary or frontend
ownership slices. The reviewer independently reran all 14 owner tests.
The complete check was repeated successfully after that repair. The fresh native
debug build passed with executable SHA-256
`fae95c8c3660e685cf8642019e00e65e2ce288e96d43ee061eb04c39c38f30d7`.

| Native flow on the final binary | Result |
| --- | --- |
| Workshop | 31/31 checks passed; no page errors |
| Chat/workspace | 24 checks passed; no page errors |
| App close | Both fixtures passed: dirty-document close/reopen and multi-project Stay open/Stop and close |

The Workshop and chat reports identify Tauri 3.0.0 with WebView2
152.0.4191.66 and persistence enabled. All flows use synthetic temporary
projects and local deterministic/mock services.

The app-close fixture records zero live model calls. It does not cover native
pending-result fault blocking; the existing core/frontend close regressions
cover that coordinator boundary.

Executed from the architecture worktree:

```powershell
pwsh -NoProfile -File scripts/desktop.ps1 -Command check
pwsh -NoProfile -File scripts/desktop.ps1 -Command spike
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-workshop-smoke.mjs
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-chat-smoke.mjs
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-app-close.mjs
openspec validate strengthen-domain-ownership --strict
```

All commands succeeded. The native flows ran sequentially after the final
runtime repair and rebuild. The final generated-module hash comparison confirms
all nine modules are unchanged from this increment's starting bytes; the
earlier lookup-contract correction remains intact.

Retained local evidence is in `.local/arch-improvement-full-check.log`,
`.local/arch-improvement-native-build.log`, the three
`.local/arch-improvement-native-*.log` files, and
`.local/arch-improvement-evidence/`. That last directory contains copies of all
three passing reports and a manifest with their hashes and the executable
identity. Native screenshots remain in `.local/native-results/`.

All 17 tasks and every acceptance row in the design are complete. Strict
OpenSpec validation and the final diff whitespace check pass. The parent
checkout's unrelated `PROJECT.md` and `.conductor/brief.txt` work is preserved.

## Compatibility and limits

No storage migration, IPC command, provider policy, actor or crate is added.
The project schema remains 40. New architecture parser dependencies reuse
already locked versions and are test-only. Existing source-epoch, receipt,
lease, historical-byte and explicit-author-action validation remains in force.

The improvement does not make every invalid authority combination impossible
by type, eliminate every raw SQLite host method, or consolidate every adoption
workflow. Those remaining targets are explicit in the current map. Local mock
qualification is separate from hosted Ubuntu, installed packaging, live
providers, human accessibility and author evaluation. No commit or push is
included in this task.

## Assessment: 8.5/10

This assessment records the ownership checkpoint. The subsequent
[material capability and command-parity increment](ARCHITECTURE_CAPABILITIES_AND_IPC.md)
implements the first two follow-on choices described below.

This is a stronger architecture than the original extraction: the important
gain is that ownership now follows operations. Conversation owns its run
queries, the document owner controls project/session transitions, library
operations use explicit navigation capabilities, and Workshop consumers cannot
assign store state directly. The boundary checks have positive and negative
fixtures, while runtime tests cover transactions, stale completions and retries.
Keep the crate decomposition, single project actor and existing receipt model.

The rating is engineering judgment, not a measured performance score. I would
prioritize narrower SQL write capabilities around one concrete adoption or
dispatch path next, retaining the caller's transaction and exact replay checks.
Actual Tauri command/argument parity would also strengthen the current naming
convention test. Typed receipt-family lookup is another candidate when a
recurring cross-domain dependency justifies it.

I would measure incremental rebuild cost and the number of owners touched by
routine feature changes before adding crates or a generic repository layer.
Broad kernel cleanup, a universal adoption abstraction and a different actor
model need evidence of a specific problem first. These are future choices, not
deferred tasks in this completed ownership plan.
