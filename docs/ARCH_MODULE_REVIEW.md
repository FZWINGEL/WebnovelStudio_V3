# Architecture branch review and fixes — 12 September 2026

Reviewed `arch-module` at `c91151b288e530f3c0e61942e603c18a4d1a56da`
against `codex/v3-persistence` at `97d0fe0aa3eb42a1648d85e944ee6e57aaa8c946`.
The fixes below are local, uncommitted changes on the architecture worktree.
This checkpoint assesses the source architecture, not release readiness.

The subsequent [ownership improvement report](ARCHITECTURE_IMPROVEMENTS.md)
records the implemented follow-on plan and updated assessment. The findings,
counts and rating below describe this earlier checkpoint.

## Six findings resolved

1. **Required checks:** normalized the extracted Rust code with the pinned
   formatter, removed redundant borrows/conversions and orphaned comments,
   and placed the view-state re-export before the test module. Strict workspace
   Clippy now passes. Most Rust files in the remediation diff changed for
   formatting; the runtime changes are limited to the reported fixes.
2. **Workshop persistence:** `WorkshopStore.flush()` rechecks dirtiness after
   joining a save drain. The regression creates an asynchronous edit in the
   previous drain's settlement window and verifies that the second write saves
   it even after the explicit flush cancels its autosave timer. The test failed
   against the old implementation and passes with the fix. Shared-loop comments
   now state the caller's responsibility to recheck dirtiness.
3. **Generated lookup contracts:** `blockIds`, `topicId`, and `reviewedMemory`
   are optional and non-null. Both omission lists are independently checked
   against a `syn` parse of Rust structs, enums, and multiline serde attributes.
   Real serde roundtrip/null-rejection tests and generated-type assertions cover
   the mismatch. UI fixtures again use the values Rust actually sends. No Rust
   wire acceptance or storage schema was changed to accommodate TypeScript.
4. **Frontend boundaries:** a TypeScript/TSX parser replaces the import regex.
   Negative/control cases cover import/export forms, dynamic and type imports,
   comments, public surfaces, shell edges, and cycles. Unresolvable dynamic
   imports fail explicitly. The existing Rolldown 1.2.7 parser is now a directly
   declared, pinned test dependency; no resolved package version changed.
5. **Rust layering:** the guard reads Cargo metadata, using actual package
   identities and all target declarations. An isolated fixture proves that
   aliases, inheritance, optional dependencies, and build/target tables cannot
   hide six prohibited edges. Development dependencies remain explicitly outside
   the production graph.
6. **Ubuntu selection:** test and Clippy commands select the workspace with only
   `webnovel-desktop` excluded. A regression guards that selection so extracted
   unit tests, architecture checks, and binding checks stay in the Linux job.

## Executed validation

| Check | Result |
| --- | --- |
| `scripts/desktop.ps1 -Command check` | Passed with Rust 1.98.1 and Node 24.20.0 |
| Rust formatting and strict workspace Clippy | Passed |
| Rust workspace tests | 943 passed, 1 intentionally ignored |
| Frontend suite | 745 passed across 77 files |
| Tooling checks | 25 passed |
| TypeScript and frontend production build | Passed; existing Vite large-chunk advisory remains |
| Fresh debug native build | Passed |
| Native Workshop, local mock | 31/31 checks passed |
| Native reviewed-memory lookup, local mock | 3/3 checks passed, zero live model calls |
| Final test-parser dependency declaration | Boundary/Workshop 28/28 and TypeScript passed |
| Independent review of the fixes | No remaining blockers found in the assigned surfaces |

The native flows verified Tauri/WebView2, save barriers, explicit adoption,
history/reopening, and retained lookup packets using synthetic projects. The
native executable SHA-256 was
`0968e9d5db87809d5a1618a556cf668459e54e32b64c9de6532eb8d75c675827`.
Its build precedes only documentation and the direct declaration of the already
locked test parser; runtime source and resolved dependency versions did not
change afterward.

Local evidence is retained in `.local/arch-fix-full-check.log`,
`.local/arch-fix-native-build.log`,
`.local/native-results/workshop/report.json`, and
`.local/native-results/memory-lookup-mock/qualification.json`.
Hosted Ubuntu, installed-package, live-provider, human accessibility, and author
evaluation were not run by this review. Their existing qualification gates remain
separate from these local results.

## Assessment: 7.5/10

Keep the decomposition. Enforced dependency directions, generated contracts,
explicit frontend surfaces, and smaller compiler stages improve navigation and
make future changes easier to review. Preserving one project actor and the
existing transaction/receipt model was the right choice for this extraction.
Layer renumbering is reasonable when it reflects actual dependencies.

The remaining weakness is ownership. Physical package boundaries are stronger
than some domain boundaries: host traits still expose raw connections, Workshop
queries the conversation's `discussion_runs` table directly, and `workspaceModel`
still centralizes several lifecycles. The original plan's authority, adoption,
provider-admission, and connection-ownership targets are not all enforced by the
new structure; existing runtime validation still supplies much of that safety.

The next changes I would prioritize, as recommendations rather than implemented
scope, are:

1. Move Workshop's specific cross-domain run queries behind conversation-owned
   readers/host operations, preserving the existing actor and transactions.
   Narrow write access around adoption and dispatch incrementally.
2. Define the state ownership and transition contract inside `workspaceModel`
   before another split. Library/project operations and active-document lifecycle
   are candidate owners; keep state with the operations that control it.
3. Publish a short current ownership map and separate the migration diary from
   current contracts. State explicitly which invariants are structural and which
   remain runtime checks or future work.

I would measure rebuild time and change-locality before adding further crates.
Broad kernel cleanup and additional generic abstractions are lower priorities
than completing these existing boundaries.
