## Why

The ownership refactor still exposes broad SQL authority at adoption helper
boundaries, and the invoke convention test checks spelling style without
proving that a called Tauri command exists or accepts its arguments. These are
the two follow-on improvements the author selected.

## What Changes

- Restrict shared ordinary-material adoption writes to an explicit capability
  borrowed from the caller's existing transaction, retaining exact target,
  checkpoint, receipt and rollback behavior.
- Derive a command contract from Rust command definitions and the actual Tauri
  handler registration, including injected versus wire arguments and naming.
- Compare every production frontend invoke with that contract using parsed
  source, rejecting unknown commands, missing or extra argument keys, and
  unsupported call shapes instead of silently skipping them.
- Add positive and negative regressions, generated-contract drift enforcement,
  focused/native qualification and current architecture documentation.

## Capabilities

### New Capabilities

None. This is an internal refactor and verification change; `skip_specs: true`
records that no product capability is added.

### Modified Capabilities

None. Existing author adoption, project transactions, schema 40, IPC behavior
and provider policy remain compatible.

## Impact

Work stays in the `arch-module` worktree and preserves all prior local fixes.
Affected areas are the documents mutation API and its conversation/Workshop
callers, the binding/contract generator, frontend contract tests, and relevant
documentation. This does not remove every raw host connection or add a new
crate, actor, repository framework, provider route, migration, commit or push.
