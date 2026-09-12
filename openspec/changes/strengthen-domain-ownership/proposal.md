## Why

The `arch-module` extraction now has enforceable package boundaries, but some
domain ownership remains implicit: Workshop reads conversation tables directly,
and the workspace model centralizes library operations and document-session
transitions. Completing those boundaries will make future changes easier to
reason about without changing the persistence or authoring contracts.

## What Changes

- Put Workshop's discussion-run reads behind conversation-owned query operations,
  including validation and recovery paths that run inside existing transactions.
- Separate library state/operations and document-session state/operations from
  shell presentation, with explicit navigation, save and identity contracts.
- Add focused regression and architectural checks for the new boundaries,
  including failed saves, uncertain outcomes and stale asynchronous completion.
- Make generated bindings reject conflicting declarations and stale extra output
  files; make Workshop state changes go through its store's actions.
- Publish a concise current ownership and invariant map, keeping the migration
  history explicitly historical and correcting obsolete source charters.
- Validate the completed change with the pinned full check and fresh native
  synthetic Workshop and workspace/conversation flows.

## Capabilities

### New Capabilities

None. This is an internal architecture refactor with regression protection and
documentation; `skip_specs: true` records that no product capability is added.

### Modified Capabilities

None. Author-visible behavior, commands and wire shapes, schema 40, receipt
identity, provider admission, and explicit author adoption remain compatible.

## Impact

Implementation is confined to the `arch-module` worktree. Rust changes affect
Workshop/conversation query seams and their core/transfer callers, with any
shared internal vocabulary kept below sibling crates. Frontend changes affect
the shell's ownership of library and document lifecycles and their tests.
Architecture checks and current documentation accompany the changes. No new
crate, actor, generic repository framework, provider route, or storage migration
is planned. Existing local review fixes are retained. No commit, push, release,
or live provider call is required by this change.
