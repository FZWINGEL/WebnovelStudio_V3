## Context

See [proposal.md](proposal.md) for motivation. The starting checkout is
`arch-module` at `c91151b`, including the completed, uncommitted ownership
increment. It has 962 passing Rust tests, 794 frontend tests and 25 tooling
tests. Those are the preceding checkpoint, not evidence for this change.

`write_material_target_at` currently accepts any `&Connection` and free-form
checkpoint reasons. Its two production callers are chat preview adoption and
Workshop adoption. Both already own an Immediate transaction and validate their
domain preview before the write. Workshop constructs decisions between writes;
changing to a batch must not reorder that behavior.

The existing invoke test scans text and checks snake/camel case. It does not
join calls to `tauri::generate_handler!` or handler signatures. The desktop uses
pinned Tauri 2.11.5 and tauri-macros 2.6.3; the local sources of those versions
establish argument conversion and injected-argument behavior. The frontend
already has the pinned rolldown source parser, and bindings already uses syn in
its tests. No new parser framework or domain crate is needed.

## Goals / Non-Goals

**Goals:** restrict the shared material-write operation's authority and lifetime;
preserve every existing adoption transaction and replay contract; detect actual
registered-command and top-level wire-argument drift in ordinary local/CI
checks; prevent unresolved syntax and stale generated data from passing silently.

**Non-Goals:** remove all raw host connections, redesign actor ownership,
invent a universal author-admission token, change chapter adoption, change IPC
or schema, infer runtime value correctness from source, or qualify live providers.

## Decisions

### 1. A transaction- and target-bound material capability

Expose a small opaque capability borrowing the caller's `&Transaction` and one
immutable `MaterialTarget`. Its consuming apply operation writes only that target
using a closed Chat/Workshop origin, which supplies the existing exact before
and after checkpoint reason strings. It exposes no connection, SQL execution,
commit, rollback or arbitrary checkpoint methods. The old connection-based
writer becomes private implementation detail.

Create and consume the capability at the existing call positions after preview
validation. Preserve target order, validation/error order, canonicalization,
head checks, checkpoint bytes and caller-owned epoch/relationship/receipt writes.
No extra connection or transaction is opened. The borrow must prevent a commit
while the capability remains in use, and a capability must not be reusable.

This protects the shared document mutation boundary. Domain orchestrators
still own their transaction and domain SQL; possession of this helper is not
proof of author intent. Merely renaming `&Connection` or adding a dereferencing
wrapper would not provide the intended restriction. A generic repository or
all-domain host rewrite would expand risk beyond this concrete seam.

Verify API restrictions with external compile-fail examples, and behavior with
transaction visibility/rollback, unchanged checkpoint reasons, exact head
conflicts and chapter refusal. Reuse the existing cross-domain rollback,
replay and protected-material integrations, including failure after material
writes but before the enclosing receipt is committed.

### 2. Generate a Rust command contract from actual registration

Use syn in the existing bindings tool to read `main.rs`, resolve the registered
handler paths through the source module tree, and extract `#[tauri::command]`
signatures. A command is callable only when present in the actual
`generate_handler!` list. Diagnose unresolved registrations, duplicate external
names, unsupported command attributes and ambiguous syntax rather than guessing.
Support relevant command/argument renames, raw identifiers, optional wire
arguments and the pinned injected Tauri types. Keep optionality and injection
separate; unknown custom injection must be made explicit, not silently omitted.

Emit `apps/desktop/src/ipc/tauriCommands.generated.json` with version 1,
`sources` entries containing repository-relative `path` and SHA-256, and sorted
`commands` entries containing external `name`, `rustPath`, `required` and
`optional` argument-name arrays. Include the source/module files needed to
resolve registration. Hashes let frontend-only tests reject a stale contract
without spawning Cargo. Rust drift testing independently regenerates the full
manifest; it is not a handwritten inventory.

Normalize CRLF to LF before source hashing on both sides. The contract must
survive normal Windows/Linux checkout line-ending differences while still
detecting source edits. Registered commands without production renderer callers
remain valid; the current 123 registrations serve 120 unique renderer calls.

Compute and validate the contract before writing generated outputs. Keep the
nine wire-type modules unchanged. The standard bindings command regenerates
the command manifest as well, and existing full checks/Ubuntu workspace tests
include its drift check without requiring a Linux desktop build.

### 3. Parse every production frontend invoke and join contracts

Replace the regex scanner with the existing TypeScript/JavaScript parser.
Resolve the imported Tauri invoke binding, including named aliases and namespace
members, and handle lexical shadowing correctly. Do not count comment/string
examples or similarly named local functions. Unresolved indirect uses, dynamic
command names, argument spreads or computed keys must be either fully resolved
by supported static syntax or explicitly rejected. They must not disappear
from coverage. Reject alternate raw Tauri bridges that evade this check.

For each call, resolve its literal command and top-level wire keys, then require
that the registered command exists, all required keys are present, and every
provided key belongs to the command. Optional keys may be omitted. Reject
statically missing/undefined required values where identifiable. Extra keys are
intentionally rejected even when the runtime deserializer would ignore them.
Nested request serialization and dynamic values remain the responsibility of
generated Rust wire types, TypeScript, serde and runtime validation.

Check source hashes before using the manifest, validate its basic structure and
uniqueness, and assert that real source coverage is nonempty. Fixtures exercise
typos that satisfy the old naming convention, unregistered handlers, missing and
extra arguments, optional omission, injection/renaming, aliases/shadowing,
comments, multiline/generic calls and unsupported dynamic syntax.

### 4. Qualification and documentation

Focused tests accompany each independent slice. An independent reviewer checks
the combined diff and test blind spots. Then run the pinned complete wrapper,
regenerate/verify contracts, build a fresh native executable, and run Workshop
and chat synthetic native flows sequentially. These exercise both material
adoption consumers and real command dispatch. Record counts, hash and limits,
and update the current map and implementation ledger. No commit or push.

## Risks / Trade-offs

- Moving validation or writes changes observable errors/receipts → preserve the
  existing per-target call positions and run rollback/replay integrations.
- Capability is overstated as universal authority → document precisely which
  public methods disappear and that orchestrator SQL remains.
- Source parsing diverges from Tauri macros → derive rules from pinned sources,
  use fixtures, fail on unsupported syntax, and retain real native dispatch.
- Generated data goes stale during frontend-only work → hash checked Rust
  inputs in frontend tests and independently regenerate in Rust drift tests.
- Aliases or computed syntax evade scanning → resolve supported forms or fail
  with an actionable diagnostic; never silently skip an invocation candidate.
- Hashing source makes some non-contract edits require regeneration → accept
  this explicit cost for standalone frontend verification and one generator.

## Migration Plan

No data migration is needed. Implement capability and contract slices in the
existing worktree, preserving prior local edits. Review and test before updating
the completion ledger. Any repair remains scoped to these boundaries. If a
rollback is required, restore only this increment's changes, retaining the
earlier fixes and all author data.
