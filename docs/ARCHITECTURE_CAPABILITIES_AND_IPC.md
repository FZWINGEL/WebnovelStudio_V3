# Material write capabilities and Tauri parity — 12 September 2026

This increment implements the two follow-on choices from the
[ownership assessment](ARCHITECTURE_IMPROVEMENTS.md). Its
[design](../openspec/changes/narrow-writes-and-check-invokes/design.md) and
[task ledger](../openspec/changes/narrow-writes-and-check-invokes/tasks.md)
are scoped to `arch-module` at `c91151b` plus the prior uncommitted fixes.

## Boundaries

The shared ordinary-material adoption write is restricted to one borrowed
transaction, one immutable target and one fixed checkpoint origin. Chat and
Workshop retain preview validation and the surrounding atomic transaction,
including domain projections, source epochs and receipts. This does not claim
that domain hosts have lost all raw SQL access or that the helper proves author
intent.

The command contract is derived from registered Rust handlers and their wire
arguments. Parsed production renderer calls are checked against that contract,
including aliases, required and optional keys, and rejected unresolved syntax.
Source hashes and an independent Rust drift check prevent stale generated data
from providing false evidence. Hashes normalize CRLF to LF for cross-platform
checkouts. Nested values and runtime authorization remain separate contracts.

The current inventory has 120 production renderer calls and 123 registered
commands. `list_documents`, `rebuild_story_index` and `revoke_story_context` are
registered without current renderer callers; that is valid. The manifest records
23 source/version inputs, 196 required argument keys and 10 optional keys.

The checker follows an explicitly supported subset of the pinned Tauri macro
and renderer syntax. It is a development drift guard, not a JavaScript security
sandbox or a Rust execution proof. Unsupported registrations, bridge use and
payload shapes require an explicit checker update. Extending this parser into
a general compiler would add more maintenance than this boundary needs.

## Evidence

The material capability passed 15 documents unit tests and four compile-fail
examples. Those examples reject a plain connection, arbitrary SQL execution,
reuse after apply, and committing before the pending capability is consumed.
Focused conversation/Workshop unit tests and adoption integrations also passed,
including grouped rollback and a downstream projection failure. The underlying
material writer is byte-identical after visibility and newline normalization.

Independent review found and verified repairs for misleading registration
locations, non-Tauri receivers, import/scope handling and computed/template raw
bridge access. Its final verdict found no remaining material blocker in the
assigned SQL, Rust command and renderer parity surfaces. Six Rust command
fixture tests and 33 frontend contract tests pass, including real source hashes
and all 120 production calls. All nine generated wire-type modules retain this
increment's starting hashes.

The pinned full check passed 978 Rust tests (one intentionally ignored), 824
frontend tests across 78 files, and 25 tooling tests. Formatting, strict
workspace Clippy, TypeScript and the production build passed. A fresh native
debug build also passed and reran TypeScript after the last scanner repair.
Vite retains its existing large-chunk advisory.

The rebuilt debug executable has SHA-256
`f9f28e1b3f311862abc0507f48657109fdce8213c10ccb3b9a7c029273aa3dc4`.
Both native traces record that exact identity. Workshop passed 31/31 checks;
chat passed 24 checks. Both reports have zero page errors and identify app
3.0.0, WebView2 152.0.4191.66 and persistence enabled. The flows ran sequentially
and exercised both material-adoption consumers, current/stale heads, grouped
adoption, replay/recovery and project reopening through real Tauri dispatch.

Executed from the architecture worktree:

```powershell
pwsh -NoProfile -File scripts/desktop.ps1 -Command check
pwsh -NoProfile -File scripts/desktop.ps1 -Command spike
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-workshop-smoke.mjs
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-chat-smoke.mjs
openspec validate narrow-writes-and-check-invokes --strict
```

Retained local evidence is in `.local/capability-parity-full-check.log`,
`.local/capability-parity-native-build.log`, the two
`.local/capability-parity-native-*.log` files, and
`.local/capability-parity-evidence/`. The evidence directory holds copied native
reports, per-flow executable/check traces and a manifest of their hashes.
Screenshots remain in `.local/native-results/`.

All ten plan tasks, strict OpenSpec validation and the final whitespace check
are complete. The parent checkout's unrelated `PROJECT.md` and
`.conductor/brief.txt` work remains intact. These results were recorded before
publication; the Git history records the subsequent commit and merge.

## Compatibility and next changes

No storage migration, command, actor, crate or provider-policy change is added;
schema 40 remains unchanged. New parser dependencies reuse locked versions in
the bindings tool. Broader domain SQL access and receipt ownership remain with
the existing transaction orchestrators.

These synthetic Windows debug flows do not establish hosted Ubuntu, installed
packaging, live-provider, screen-reader, physical-keyboard or author-trial
qualification. The Workshop and chat reports retain their individual limits.

The next useful improvement should follow a demonstrated recurring problem:
measure change locality and incremental rebuild cost, then consider typed
receipt-family lookup or another concrete write capability. Keep the single
project actor and the existing crate boundaries. Avoid a universal adoption
framework or general-purpose source resolver without evidence that it reduces
maintenance.
