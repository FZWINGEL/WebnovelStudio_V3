# V3 target architecture — modular decomposition

**Status:** proposed. This document defines a target structure; it does not by itself
authorise a rewrite. It is written against `97d0fe0` and every measurement in it was taken
from that tree.

**Relationship to existing documents.** This replaces nothing. The
[refined architecture](V3_ARCHITECTURE_REFINED.md) remains the product-level architecture,
the [ADRs](README.md) remain the behavioural contracts, and
[implementation status](IMPLEMENTATION_STATUS.md) remains the evidence ledger. This document
addresses one thing those do not: **how the source is decomposed, and what stops it
decaying again.**

---

## 1. Why

The product works. The problem is that it has become one thing.

| Layer | Lines | Shape |
|---|---|---|
| `crates/core` | 64,875 src + 47,296 test | **one crate**, 7 public modules, 21 files over 1,500 lines |
| `apps/desktop/src-tauri` | 15,200 / 33 files | 123 commands, 6 managed states, no `lib.rs` |
| `apps/desktop/src` | 27,537 / 188 files | 255 hand-written IPC type mirrors |

Nothing in the build enforces a boundary between "the editor contract" and "backup
restoration". There is no mechanism that can fail when a wrong dependency appears, so none
did.

### 1.1 Measured defects

Every item below is a fact about the tree, not an impression.

**D1 — One crate holds the entire domain.** `crates/core` is the only crate under
`crates/`. It exposes 7 modules (`crates/core/src/lib.rs:13-20`). Any change to storage
recompiles documents, providers, context, chat and workshop.

**D2 — `ProjectSession` is a god type.** `crates/core/src/projects.rs:405` — an mpsc actor
whose `Command` enum has ~28 variants (`projects.rs:~340-389`) and whose `impl` exposes
**32 `pub fn`** (`projects.rs:411-877`). Every project operation in the codebase routes
through it.

**D3 — `projects.rs` is a namespace, not a module.** `use super::*;` appears in **28 files**
under `projects/`. No submodule can be extracted without first splitting the file it glob-
imports.

**D4 — A real dependency cycle.** `crates/core/src/storage/mod.rs:1` imports `CoreError`
from `projects`, while `crates/core/src/projects.rs:3` imports `storage`. Storage cannot
become a crate until `CoreError` moves out of `projects.rs:43`.

**D5 — Three files each span three or four subsystems.**

| Lines | File | Subsystems touched |
|---|---|---|
| 4,149 | `context/packet.rs` | 12 sibling `context` modules + `documents::{Endpoint, ScopeGrant, ScopeKind, ScopeValidationRequest, validate_scope}` + `projects::{project_chat_output, story_context, workshop_generation}` (`context/packet.rs:10-55`) |
| 4,605 | `projects/discussions.rs` | `context::{continuation, lookup, packet}` + `documents` + `projects::{context_packets, discussion_lookup, story_context, workshop_generation}` + `providers::http_request` (`discussions.rs:11-29`) |
| 4,135 | `projects/workshop.rs` | `discussions::DiscussionRun` (`workshop.rs:9`) + `context` + `documents` |

**D6 — The frontend contract is unverified duplication.** `apps/desktop/src/ipc/*.ts` holds
**255 `export interface`/`export type` declarations across 22 files** and **120 `invoke`
call sites**. Rust types are mirrored by hand (`crates/core/src/projects.rs:94` ↔
`apps/desktop/src/ipc/projects.ts:7-14`); there is no codegen (`specta|ts-rs|tauri-specta`
match nothing in `src-tauri`). **Only one of the 22 files has a test**
(`createIntent.test.ts`, 96 lines); the other 1,229 lines of contract surface are untested,
so drift is caught at runtime or not at all.

**D7 — `Workspace.tsx` is a god component.** 1,206 lines, **22 `useState`**, 34 hook calls,
importing 15 symbols from 6 sibling directories. It owns library CRUD, project lifecycle,
document CRUD, tab and workspace routing, export preparation, app-close orchestration, V2
import, story bible and chat-adoption bridging. `ProjectConversation.tsx` receives **14
props** (11 callbacks) and is handed a fully-constructed `<Writer>` element as a ReactNode
(`Workspace.tsx:1182`).

**D8 — Four hand-rolled copies of one store pattern.** The `listeners` Set + `flight`
promise + `pending` retry slot pattern is reimplemented in `editor/session.ts:58-65`,
`chat/conversationStore.ts:114-138`, `workshop/store.ts:32-39` and `assistant/composer.ts:14-15`.
`WorkshopStore` and `ProjectConversationStore` are near-isomorphic.

**D9 — Helpers duplicated across modules.** `sameHead` ×6, `errorCode` ×5, `errorText` ×6,
bigint version parsing ×2.

**D10 — Three vocabularies collide on `W`.** `W0`–`W8` are work packages, `W30`-style
identifiers in `PRODUCT.md:120-123` are native smoke-test group numbers, and the chat-first
ledger uses `CF0`–`CF6`. Nothing distinguishes them lexically.

---

## 2. Design principles

**P1 — Dependencies point one way, and the build proves it.**
The crate graph is acyclic by construction and a test fails if an edge violates the layering.
A boundary that nothing enforces is a comment.

**P2 — The five product invariants become structural, not conventional.**
These are the spine of the product (see §1 of the functional inventory and `PRODUCT.md`):

| Invariant | Today | Target |
|---|---|---|
| **Authority separation** — documents/revisions are story authority; digests, summaries, lookups, guidance and chat output are not | Convention across ADRs | Generated-content types cannot be passed where an authority type is expected — enforced by the type system in `wns-story` / `wns-context` |
| **Explicit Apply only** — every AI-produced change commits atomically with receipt, before/after revisions, source-epoch advance | Per-call-site discipline | One `Adoption` entry point in `wns-conversation`; no other path can write a document |
| **No paid call without an explicit author action** | Reviewed per feature | Provider dispatch sits behind a capability token minted only in a command handler, never in read paths |
| **Stale-basis fencing** — source epoch + policy + operation namespace; recovery never replays a request | Implemented broadly | `wns-kernel` owns `SourceEpoch`; every packet carries it by type |
| **Byte-compatibility of historical records** — migrations must not rewrite historical packet bytes | Implemented | `wns-storage` owns migrations alone; nothing above L1 can open a raw connection |

**P3 — Tests move with their code.** The 77 registered integration suites stay runnable at
every step of the migration. A step that cannot keep the suite green is not a step.

**P4 — The migration is a sequence of extractions, never a rewrite.** Every step is a
`cargo check`-and-test-green commit. There is no branch where the product does not build.

---

## 3. Target architecture — Rust

### 3.1 Crate graph

```
L6  wns-app           Tauri shell: thin command modules, one per feature
                      ────────────────────────────────────────────────────────
L5  wns-transfer      backup · recover · duplicate · export · v2-import · recovery copy
    wns-library       library index + app preferences (CAS on revision)
    wns-conversation  discussions · project chat · proposals · guidance · adoption
    wns-workshop      six-lens Workshop, typed candidates, generation
                      ────────────────────────────────────────────────────────
L4  wns-story         memory · reviewed story · story context · evidence/promise history
                      · accepted summaries · character knowledge
                      ────────────────────────────────────────────────────────
L3  wns-context       packet compiler · eligibility · lookup · receipts · inspector
                      ────────────────────────────────────────────────────────
L2  wns-documents     document model · scope · structured blocks · editor contract
                      · revision history and restore
                      ────────────────────────────────────────────────────────
L1  wns-storage       SQLite · the 40 migrations · durability · backups-before-upgrade
                      · typed row access (documents, revisions, receipts)
    wns-providers     Provider port + codex-exec / app-server / claude / http / mock
                      ────────────────────────────────────────────────────────
L0  wns-kernel        CoreError/CoreResult · sha256 · canonical JSON · W0 snapshot validator
                      · record vocabulary · ids · head/hash predicates
```

**The arrow rule:** a crate may depend only on strictly lower layers. `wns-app` depends on
everything; nothing depends on `wns-app`. Siblings at one layer must not depend on each other —
at L1, `wns-storage` and `wns-providers` are mutually independent, and `wns-documents` is no
longer their sibling: it rose to L2 when revision history moved into it.

**The layer numbers are ordinals in a partial order, not a fixed taxonomy.** They were
renumbered once already, when `wns-documents` gained its dependency on `wns-storage` and every
layer above it shifted up one. What must hold is the relative order and the absence of sideways
edges; `crates/architecture` enforces exactly that, and it caught the renumbering this section
describes by failing on the new L2→L2 edge by name.

### 3.2 Why these boundaries

They are drawn on **change axes**, not on file proximity. Each crate answers "what would
change together?":

- `wns-kernel` is forced. `validate_snapshot_json` and `sha256_hex` (`lib.rs:44`) are pulled
  in by `projects.rs:3`, `transfer.rs:14` and `context/packet.rs:56`. `CoreError`
  (`projects.rs:43`) must move here to break D4. Nothing can be extracted before this.
- `wns-storage` splits from `wns-documents` because schema evolution and document semantics
  change for unrelated reasons, and because D4 blocks storage from being standalone.
- `wns-providers` is already nearly clean — recon found only 7 `context` references and 2
  `projects` references across 16,116 lines, and `providers/mod.rs:1-5` plus `adapter.rs:1-5`
  already document independence as an intent. This is the cheapest large extraction.
- `wns-context` is split from `wns-story` because packet compilation (deterministic, bounded,
  byte-exact) and story semantics (reviewed prefixes, evidence, promises) fail for different
  reasons and are tested by different suites.
- `wns-conversation` and `wns-workshop` are siblings, not a chain — but today
  `workshop.rs:9` imports `discussions::DiscussionRun`. That single edge must be inverted
  (a shared run/settlement type moves down to `wns-story` or a small shared type) before
  they can be siblings.

### 3.3 Enforcement

Without enforcement this document is a suggestion. Two mechanisms:

1. **A layering test** in the workspace, run in CI: read each crate's `Cargo.toml`, assert
   every dependency edge respects the layer table, assert no sibling edge exists. Cheap,
   deterministic, and it fails the build — which is the only thing that has ever worked here.
2. **`cargo deny`** or an equivalent, to catch a *new* dependency appearing in a low layer
   (e.g. `reqwest` in `wns-documents`).

### 3.4 Splitting the three 4,000-line files

These are the hard part of the migration and the reason D5 matters.

**`context/packet.rs` (4,149) → `wns-context`.** First split the file itself into
compilation stages (select → budget → freeze → receipt), then remove the `projects::*`
imports at `packet.rs:48-55` by inverting them: the packet compiler should receive
already-materialised inputs, not reach into `project_chat_output`, `story_context` and
`workshop_generation`. That inversion is the single highest-value change in the Rust tree,
because it is what makes `wns-context` a real layer instead of a façade over `projects`.

**`projects/discussions.rs` (4,605) → `wns-conversation`.** Four concerns in one file: run
lifecycle and settlement, recovery/lost-acknowledgment, lookup invocation, and packet
assembly glue. Split along those lines; the packet glue is what moves down to `wns-context`.

**`projects/workshop.rs` (4,135) → `wns-workshop`.** Six-lens domain logic versus persistence
versus generation orchestration. The generation half already lives in
`workshop_generation.rs` (1,895).

### 3.5 Replacing `ProjectSession`

D2 is the deepest coupling. The target keeps **one** actor per project — the concurrency
model is sound and the migrations, durability and lost-acknowledgment behaviour depend on it —
but replaces the 32-method façade with **per-concern facades over the same handle**:

```
wns-documents  →  DocumentApi   (open, save, checkpoint, restore, head, revisions)
wns-context    →  ContextApi    (freeze, retrieve, packet, receipt)
wns-story      →  StoryApi      (reviewed prefix, evidence, promises, knowledge)
wns-conversation → ConversationApi (runs, drafts, adoption)
wns-transfer   →  TransferApi   (backup, recover, duplicate, export)
```

Each facade holds the same `Arc<Handle>` and issues its own `Command` variants. Callers stop
depending on a type that knows about everything. The actor's command enum is partitioned
along the same lines, which also makes the enum's ~28 variants comprehensible.

**This does not change the runtime behaviour the tests assert** — the actor, the channel and
the ordering guarantees are untouched. It is an interface change, which is exactly why it is
worth doing before any of the 4,000-line splits.

### 3.6 `wns-app` (the Tauri shell)

123 commands today, all registered in one `generate_handler!` (`main.rs:173-297`), with six
`manage()` calls and no `AppState` (`main.rs:93-118`). Target: one module per feature crate
(`commands/documents.rs`, `commands/context.rs`, …) and **one** `AppState` owning the
facades. `discussion_commands.rs` (2,232 lines / 12 commands) and `provider_runtime.rs`
(1,647) are the two files to break up first.

### 3.7 `contracts/`

The four golden fixtures are read at *runtime* by relative path from both Rust
(`crates/core/src/lib.rs:~482` reaches `../../contracts/`) and TypeScript. A crate split
breaks those relative paths. Target: promote `contracts/` to a workspace member crate that
ships the fixtures as data, so consumers reference `contracts::W0_SNAPSHOT_GOLDEN` rather
than a `../../../` path. This also gives the cross-language contract a testable home.

---

## 4. Target architecture — frontend

### 4.1 Feature slices

```
src/
  kernel/          errors · result · canonicalJson · version/bigint · sameHead
  ipc/             GENERATED bindings (see 4.4) + thin typed wrappers
  features/
    editor/        DocumentSession, Tiptap model, snapshot client
    chat/          conversation store, draft review, adoption
    workshop/      six-lens board, candidates, decisions
    assistant/     feedback, guidance, context inspector, proposal review
    story/         story bible, aliases, chapter memory, evidence views
    providers/     model picker, endpoints, transport settings
    library/       project list, create/open/duplicate/backup, V2 import
  shell/           routing, layout, composition ONLY
```

A feature may import from `kernel/`, `ipc/` and its own directory. **Cross-feature imports
go through a feature's public `index.ts`**, and the lint rule enforces it. Today
`shell/Writer.tsx` reaches into three features (D7); after the split that becomes an explicit,
reviewable edge.

### 4.2 `kernel/`

Directly removes D9: one `sameHead`, one `errorCode`, one `errorText`, one bigint version
parser. These are not cosmetic — `sameHead` appearing six times means six places a
comparison could be subtly wrong about document identity.

### 4.3 One save loop

**Delivered as `kernel/saveLoop.ts`.** D8 said four stores converge on one
listener/flight/pending pattern. Reading them rather than counting them changed the
conclusion: they do **not** converge on one store shape. `ComposerSession` has no listener
set at all — it is not subscribable — and it has no debounce timer, while `WorkshopStore`
has both. What they genuinely share is narrower and more important: the **single-flight save
loop with lost-acknowledgment retention**.

The retention rule is the part worth having once:

> A write that fails without a definitive answer must keep the exact payload and operation
> id it sent. A retry under a fresh operation id is a second mutation of the same intent.

`createSaveLoop` owns that loop and the rule. Two asymmetries are preserved deliberately
rather than flattened, because each is load-bearing:

- `discardOn` is opt-in. `WorkshopStore` declares the validation codes that provably precede
  a transaction — retaining those bytes would stop the author saving their correction.
  `ComposerSession` declares none, so a protocol mismatch retains the payload for
  reconciliation. A shared default in either direction would be wrong for one of them.
- The loop joins an in-flight drain but does **not** re-run after it. `drain` already
  re-checks `isDirty` between writes; re-running in the loop would livelock whenever a
  `capture` legitimately returns `null` for a still-dirty store. `ComposerSession`, which
  has no debounce timer, re-checks itself in `save()` instead.

Verified: `tsc --noEmit` clean; the 16 store tests (`workshop/store.test.ts`,
`assistant/composer.test.ts`) pass, including the lost-ack retention, late-acknowledgment and
post-refusal-correction cases that pin this rule, and the full frontend suite is unchanged.

### 4.4 Generated IPC bindings

Directly removes D6, the largest correctness risk in the tree.

Rust types become the single source of truth via `specta`/`ts-rs`, generated into
`src/ipc/generated/`. The 255 hand-written mirrors are replaced. This is not only
deduplication: **the 1,229 lines of currently untested contract surface acquire a test** —
a generated-binding drift check in CI that fails when Rust changes without regeneration.

Two things stay hand-written and must be tested: the response-hash cross-check
(`ipc/native.ts:12-13` recomputes `canonicalJson` + `bodyHash`) and the argument naming
convention (commands are snake_case, arguments camelCase — ad-hoc today).

### 4.5 Shell

`Workspace.tsx` (D7) reduces to routing and layout. Library CRUD moves to
`features/library`, document CRUD to `features/editor`, app-close orchestration to its own
module, chat adoption bridging to `features/chat`. The `<Writer>`-as-ReactNode injection
(`Workspace.tsx:1182`) is replaced by the chat feature rendering the editor itself through
the editor feature's public interface.

---

## 5. Migration sequence

Each step ends with `cargo check`/`cargo test` and `vitest` green, and is its own commit.
Steps 0–2 are delivered in this branch — see §8 for what was built and the evidence recorded.

| # | Step | Proves |
|---|---|---|
| 0 | Workspace skeleton: create the crates, layering test, `cargo check` | The graph is buildable and the rule is enforceable (P1) |
| 1 | **Extract `wns-kernel`** — move `CoreError`/`CoreResult` out of `projects.rs:43`, plus `sha256_hex`/`validate_snapshot_json` | Breaks D4; unblocks every later extraction |
| 2 | **Extract `wns-storage`** — `storage/` + the 40 SQL migrations, depending only on `wns-kernel` | The cycle is gone and storage is genuinely standalone |
| 3 | Extract `wns-documents` — `documents/` + the editor contract | Leaf layer complete |
| 4 | Extract `wns-providers` — 16,116 lines, only 9 references to invert | The cheapest large win; validates the pattern at scale |
| 5 | Introduce the per-concern facades over `ProjectSession` (§3.5) | D2 addressed without touching runtime behaviour |
| 6 | Split `context/packet.rs`; extract `wns-context` | D5's worst case |
| 7 | Extract `wns-story`, then `wns-conversation` and `wns-workshop` (invert the `workshop.rs:9` edge first) | Middle layers |
| 8 | Extract `wns-transfer`, `wns-library`; promote `contracts/` | Top layer |
| 9 | `wns-app` restructuring: one command module per feature, single `AppState` | Shell becomes thin |
| 10 | Frontend: `kernel/`, `createStore`, generated IPC, feature slices, shell reduction | D6–D9 |

Frontend steps are independent of the Rust steps after step 0 and can be interleaved.

---

## 6. Risk register

| Risk | Why it matters | Mitigation |
|---|---|---|
| **Extraction breaks byte-compatibility** | Migrations must not rewrite historical packet bytes/hashes (`wns-storage` reader floors at `mod.rs:181-229`) | Storage extraction is a *move*, not a change. Any migration edit is out of scope for this migration and flagged separately |
| **The 77-suite integration funnel complicates moves** | `crates/core/tests/integration.rs` lists all suites in a `suites!` macro and asserts the registration list against `read_dir` | Suite registration updates are part of each extraction step, never batched |
| **`ProjectSession` facades change behaviour subtly** | The actor's ordering and lost-ack behaviour is load-bearing | Facades are an interface change only; the actor, channel and command ordering are untouched. Existing tests are the contract |
| **Sibling layers turn out to need each other** | e.g. the `workshop → discussions` edge | The edge is inverted before the split, not after. If a proposed boundary needs a sibling edge, the boundary is wrong — redraw it |
| **Enforcement test is bypassed** | A rule that can be skipped will be | The layering test runs in the same CI job as the build |
| **This becomes a rewrite** | 108k lines; a rewrite loses the 1,639-test evidence base | P4: extractions only, every commit green, no long-lived branch |

---

## 7. Deliberately out of scope

- **No behaviour change.** This is a structural migration; feature work is unaffected.
- **No schema change.** Schema 40 stays 40.
- **No test deletion.** The ~926 Rust test functions and 713 frontend tests move with their
  code.
- **No framework change.** Tiptap, Tauri, React and the provider transports stay.
- **The three-vocabulary problem (D10)** is named but not solved here; it needs a naming
  decision, not a structural one.

---

## 8. What this branch delivers

Steps 0–2 of §5, plus the enforcement mechanism of §3.3.

**The skeleton.** Eleven crates: `wns-kernel`, `wns-storage`, `wns-documents`,
`wns-providers`, `wns-context`, `wns-story`, `wns-conversation`, `wns-workshop`,
`wns-transfer`, `wns-library`, and `wns-architecture`. The eight unported crates carry no
code — each `lib.rs` states its responsibility, why the boundary exists, which current files
will land in it, and what must be inverted first. The skeleton doubles as the migration map.

**Two real extractions.**

*Step 1 — `wns-kernel`.* `CoreError`/`CoreResult`/`Head` moved out of `projects.rs`, and the W0
snapshot validator plus `sha256_hex` and `canonicalize_value` out of `lib.rs`. `webnovel-core`
re-exports all of them at their old paths, which is what keeps every existing import —
including `webnovel_core::projects::{CoreError, CoreResult, Head}` in the Tauri shell and the
28 `use super::*` globs — resolving unchanged. This is the step that breaks D4.

Two changes were needed to make the move compile, and both are recorded rather than hidden:

- `CoreError::uncertain` took a `rusqlite::Error`. It now takes `impl Display`, so the kernel
  never has to name a persistence error type. Every call site is a
  `map_err(CoreError::uncertain)` and still infers unchanged.
- `CoreError` keeps its `From<rusqlite::Error>` and `From<std::io::Error>` impls, so the
  kernel does depend on `rusqlite`. An error type that must convert from every layer's error
  is inherently coupled to those layers, and the orphan rule is why the conversions cannot
  live in the layers that produce them. This is a deliberate deviation from "L0 is
  dependency-free" and the one thing a reviewer should push back on if they disagree.

*Step 2 — `wns-storage`.* The storage module and all 35 migration SQL files moved to a crate
depending only on `wns-kernel`. The migration chain is **move-only** — schema 40 stays 40,
the SQL and their `if version < N` ordering are byte-identical, because the historical-packet
byte guarantee depends on migrations never being rewritten. `webnovel-core` aliases the crate
as `storage`, so `crate::storage::{configure, migrate, LATEST_SCHEMA_VERSION}` resolves as
before.

*Step 3 — `wns-documents`.* `documents/` (scope validation, structured blocks) moved to a
crate depending only on `wns-kernel`. Its own private `sha256_hex` copy was deleted in favour
of the kernel's, taking the tree from three copies of that function to one.

*Step 4 — `wns-providers`.* The 16,116-line provider surface moved to a crate depending only
on `wns-kernel`. This step required the inversion §3.2 predicted, and it turned out to be a
**real cycle rather than a one-way edge**: `context/packet.rs` sourced its profile constants
from the provider modules (`CODEX_PROFILE_VERSION`, `CLAUDE_INPUT_LIMIT_BYTES`, …) while the
provider adapters reached *up* into the compiler for `ProviderBinding`. Two changes break it:

- The provider contract vocabulary — the profile constants and limits, `ProviderBinding`,
  `HttpProviderBinding`, `HttpResponseFormat`, `ProviderRuntimeIdentity`, `PacketMessage`,
  `PacketOptions`, and the two helpers they need — moved down into
  `wns-providers::vocabulary`. `context/packet.rs` re-exports every item at its historical
  path, so all 158 `ProviderBinding` references and every packet's serialized layout are
  unchanged. The byte-compatibility suite is what proves that.
- `openai_compatible::stream_packet_async` took `&CompiledPacket`. `CompiledPacket` carries a
  `PacketReceipt`, which is the compiler's own product and belongs at L2, so reaching up for it
  is what made providers depend on context. It now takes `&[PacketMessage]` and
  `&PacketOptions` — the two fields it actually read. Its two callers pass those.

`packet.rs` went from 4,149 to 3,462 lines. Extracting the vocabulary required moving five
disjoint line ranges out of a byte-critical file plus two shared helpers, so the ranges were
lifted with `sed` on exact line numbers rather than retyped, and the serialization tests are
the check on whether that was done correctly.

**One thing this step exposed.** `wns-providers` now has a public surface where it had
`pub(crate)`. `webnovel-core/src/library.rs` reaches into provider internals — catalog
construction, endpoint-profile validation, preference validation — for production logic, not
tests. Those items are promoted to `pub`; the honest fix is step 8, where `wns-library` takes
that logic with it.

**Prerequisite step — `projects.rs` split (D3).** Every remaining crate is gated behind this,
so it was done next rather than in sequence. `projects.rs` was a *namespace*, not a module: it
held the record types, the actor, the session façade and the persistence helpers in 2,709
lines, and 28 files under `projects/` reached all of it through `use super::*`. Nothing could
be lifted out until those concerns were separated.

It is now `projects/records.rs` (253 lines — the record types) and `projects/session.rs`
(488 lines — the actor, `Command`, `Handle`, `ProjectSession`), with `projects.rs` down to
2,024 lines holding the submodule declarations, the re-exports, the persistence helpers and
the `OwnedProject` actor implementation. **The re-export is what makes this a safe split**: `pub use records::*; pub use
session::*;` means `use super::*` and `crate::projects::{…}` resolve exactly as before, so not
one of the 28 dependent files had to change.

Three items needed wider visibility to cross the new module boundary — `Command` and `Reply`
became `pub(crate)`, and `ProjectSession::request` did too, since 94 call sites in sibling
submodules use it. **That is the finding worth keeping**: the session façade is not merely
large, it is the channel every other module writes through. Widening three visibilities to
split one file is the measurement of D2, and it is why step 5 — replacing that façade with
per-concern ones — has to come before the middle layers can move.

**Step 5 begun — per-concern facades.** The Workshop concern is delivered
(`projects/workshop_api.rs`): six methods instead of 28, holding the **same** `Arc<Handle>` and
issuing its own command variants. `ProjectSession::workshop()` returns it. Nothing about the
actor, the 64-slot channel, the reply semantics or the ordering guarantees changed — it is an
interface change, which is exactly why the existing suite is the whole contract.

The first count of this was wrong, and the way it was wrong is the useful part. Grepping
`crates/core/src` and `apps/desktop/src-tauri` put the Workshop at **13 call sites** and made it
look like the smallest coherent concern. The migration then failed to compile with 205 errors:
the real number, counting `crates/core/tests/` and `crates/core/examples/`, was **~200**. The
grep enumerated the callers of the *thing* and forgot that the test tree is a caller — the exact
omission the call-site sweep exists to prevent, made while performing one.

Accurate counts, all targets included:

| Concern | Methods | Call sites |
|---|---|---|
| Documents (attach, attach_snapshot, open, create_document, document(s), save, checkpoint, history, reconcile, view state) | 11 | **~824** |
| Workshop | 6 | ~200 ✅ migrated |
| Project lifecycle (create, metadata, rename) | 3 | ~82 |
| Context (source epoch) | 1 | 66 |
| Background work (start/stop/interrupt) | 3 | 29 |
| Storage info | 1 | 3 |

Widening `Handle`, its `queue` field, `Command` and `Reply` to `pub(crate)` is what a facade
costs. That is the honest price of D2 and it does not shrink until the last group moves.

**The WorkApi facade** (`projects/work_api.rs`) followed: three methods — `census`, `stop`,
`interrupt` — for the background-work census, which is read on two unrelated paths (the
project-chat write path and the app-close sequence) that each need three operations out of 28.

**The DocumentApi facade** (`projects/document_api.rs`) is the largest and was done last for a
reason. Eleven of the 28 methods are document operations — attach, create, read, list, save,
checkpoint, history, reconcile, the two view-state calls and the attachment snapshot — and they
are the ones that actually widen the session's surface. 692 call sites across 62 files moved.

It is also the only facade whose migration could not rely on the rewrite being complete.
`attach` and `save` are generic names; `OwnedProject` implements every method here, and so do
test fixtures — `f.save(...)` and `fixture.reconcile(...)` are *different types with the same
names*, living in the same files as the session's callers. A rewrite keyed on the method name
would have corrupted them.

So the order was inverted: **the session's methods were removed first**, and the compiler then
enumerated every remaining call site. A missed receiver or a wrong receiver both became build
errors rather than silent mis-dispatch. That loop found what four passes of pattern-matching had
missed — the chained form (`project\n  .documents(…)`, whose receiver is on a previous line),
`projects.rs`'s own `#[cfg(test)]` module, `transfer.rs`, and two method-call receivers
(`fixture.project().attach(…)`). It also caught two over-applications where a later pattern
re-matched its own output (`.documents().list().list(…)`), which the compiler reported as a
0-argument call.

This is the reusable part, and it supersedes the "a method name does not identify a type"
warning from the earlier facades: **when the call sites cannot be enumerated reliably, delete
the old surface and let the compiler enumerate them.** Pattern-matching is a guess about
completeness; removing the method makes completeness the compiler's problem, and the compiler
does not forget the test tree.

**The ProjectApi facade** (`projects/project_api.rs`) covers project-level lifecycle and
inspection — `metadata`, the two renames, and the storage report — for the library and transfer
paths that need nothing else. Four methods instead of 28. `OwnedProject` implements all four as
well and calls them on itself, so the same exclusion discipline applied: those sites were
verified untouched by count before and after.

**Two mistakes were made doing this, and they are the same mistake twice.** The first count of
Workshop call sites forgot the test tree is a caller. The next one ran a blanket
`find crates apps -exec sed` for the Work methods, which also rewrote the actor's own dispatch
block in `session.rs` — where `project.background_work()` is a method on `OwnedProject`, a
*different type* that happens to share the name. The compiler caught both.

The shared cause is worth naming: **a method name does not identify a type.** Every enumeration
of call sites in this migration has to answer two questions, not one — which receivers exist,
and what type each one is. Grepping a method name gets you the first and silently guesses the
second, and the guess is invisible until something compiles or does not.

**The ContextApi facade** (`projects/context_api.rs`) is where that lesson was applied rather
than learned: `OwnedProject` also has `context_source_epoch`, called as
`self.context_source_epoch()` in `context_packets.rs` and `discussions.rs`, textually identical
to the session's. Those three files were named as excluded *before* the rewrite and verified
unchanged after it. The two are genuinely different operations — one runs on the actor thread
against the live connection, the other asks the actor over the channel — and the facade doc
says so, because the next person to grep for that name will hit the same fork.

**Enforcement.** `crates/architecture` asserts, in CI-able tests: every layered crate exists
with a manifest; every layered crate is a workspace member; no crate depends on a sibling or a
higher layer; and no layered crate depends on `webnovel-core`. It ships a deliberately
violating fixture manifest and asserts the parser surfaces both violation kinds, so the check
cannot quietly become a no-op. It adds no dependency of its own — the manifest parsing is
`std`-only, so the checker cannot itself violate the graph it enforces.

**Evidence.**

| Check | Result |
|---|---|
| `cargo check --workspace` | clean |
| `cargo test -p webnovel-core` | **719 passed, 0 failed** — unchanged by both extractions |
| `cargo test -p wns-architecture` | 5 passed, including the can-fail probe |

**What the extraction surfaced.** The storage move broke `crates/core/tests/metadata.rs:49`,
which built a legacy v1 project fixture with
`include_str!("../src/storage/001_projects.sql")` — a test reaching across crate boundaries
into another crate's source tree by filesystem path. That coupling was invisible while both
lived in one crate and is a boundary violation the moment they do not. The fix is not to
repoint the path but to expose the script through the owning crate
(`wns_storage::SCHEMA_001_PROJECTS_SQL`), so the dependency is named rather than assumed.
Expect more of this: every extraction of a module with adjacent tests will surface path-level
couplings that no `use` statement ever revealed.

**Frontend step — the kernel (`apps/desktop/src/kernel/`).** The frontend analogue of the
`wns-kernel` extraction: `sameHead`, `sameDocumentHead`, `errorCode` and `errorText` now exist
once instead of across sixteen module-local copies. `errorTextFor(fallback)` binds the shared
reader to a module's own message, which is what let eight modules drop their local copy with
**zero call-site edits** — there are 47 `errorText` call sites and a 47-site mechanical diff is
exactly where behaviour changes silently.

Three copies were deliberately **not** merged, because they are different contracts rather
than duplicates, and each is named in `kernel/errors.ts`:

* `chat/useDraftReviewContext.ts` also accepts a bare `{ message }` object that is not an
  `Error`; folding that in would change the fallback at every other site.
* `story/DocumentAliases.tsx` prefers `Error.message` **over** `detail` — the opposite
  precedence — because its call sites embed the text in a sentence about a save possibly
  having completed.
* `shell/Workspace.tsx`'s fallback is derived from the value (`String(error)`) rather than a
  constant, so it has no fixed message to bind.

Similarly, `sameHead` was **six copies of two different functions**: five compared two non-null
heads and `ReviewPanel`'s tolerated a null on the left. Merging them would have been a
null-safety regression, so both contracts are kept explicitly separate.

**Verified:** `tsc --noEmit` clean; `vitest` **713 passed across 74 files**, unchanged.

---

## 9. The test suite is not where the debt is

A full audit of `crates/core/tests/` (78 files, ~47k lines) and the inline tests was run against
the hypothesis that a large share of it tests superseded functionality. **It found zero
genuinely dead tests.** Every version-numbered test — roughly 48 test functions across 25 files
— exercises today's live migration chain, today's live reader-floor validation, or today's live
byte-compatibility contract. The old version *numbers* are inputs to current code, not
artefacts of old code.

Three things make this worth stating outright, because each is a trap for a future cleanup:

* **`create_v1_project` (`tests/metadata.rs:45`) is not a stale fixture.** It builds a schema-1
  database with `wns_storage::SCHEMA_001_PROJECTS_SQL` — a *production* constant, which is what
  makes the fixture truthful by construction. Its two consumers assert that a schema-1 project
  opens under today's code without losing the author's writing, and that a **failed** migration
  rolls back, leaves the new column absent, and retains exactly one pre-upgrade backup. The
  second is failure coverage for the most destructive operation in the product.
* **`tests/support/schema.rs` is not stale data.** It is an *inverse-migration toolkit*: it
  strips columns, tables and triggers from a **current** database so it can be truthfully
  presented as an older one. Twelve files use it at 52 call sites. It is a precision instrument.
* **`tests/v2_import.rs` is the highest-risk misread.** "V2" is the *previous product's* schema,
  not an old V3 version. The importer is live: `v2_import.rs` is wired through
  `library.rs:858 import_v2` and `:1022 resume_v2_import`, documented in `PRODUCT.md:201` as a
  shipped native action. Deleting those 16 tests would remove coverage for a shipped feature.

What keeps the tree honest is structural, not diligence: `tests/integration.rs:11-25` compiles
all files into one binary and asserts `registered == discovered`, so a test file cannot be
orphaned or silently emptied.

Verified alongside: **exactly one** `#[ignore]` in the whole tree —
`projects::tests::crash_child`, which carries the reason
`"Subprocess target for kill_after_commit_before_ack_recovers_once"` and is spawned by its
sibling test rather than skipped. It is a fixture, not a disabled test. (An earlier pass of
this audit reported zero; that count grepped the bare `#[ignore]` form and missed the
reason-string form. Recorded because the correction is the point of an audit.) Also verified:
zero `todo!`/`unimplemented!`, and no test pinning a retired wire contract.

**One test is timing-sensitive, found by accident.** During step 6,
`codex_app_server::completed_thread_threshold_recycles_idle_connection` failed
once in a full run (`left: 1, right: 0`) and passed both in isolation and on the
next full run. It is unrelated to that change, which moved string constants. The
cause is visible in the test: it performs 128 sequential start/collect
round-trips against a real child process and then asserts `active_count() == 0`
immediately, with no wait for the last request to finish settling. Under the
parallel load of a full suite, that read can win the race.

This matters beyond the one test. Every step of this migration was verified by
`719 passed / 0 failed`, and that figure is a strong signal but not a proof — it
is a suite containing at least one test that can fail for reasons unrelated to
the change under test. A green run is evidence; a red run of *this* test, on a
change that does not touch the app-server transport, is not.

**Fixed.** The assertion was racing the recycle rather than waiting for it, so
it now polls `active_count` against a deadline using the same
`Instant::now() + Duration` pattern `collect` already uses in that file, and
falls through to the plain assertions for `health()` and the reservation
refusal once the recycle has actually happened. Documenting a flake is not the
same as fixing it, and a known-flaky gate is worse than either: it trains
whoever reads the next red run to explain it away.

**The one real instance of what was being looked for is documentation, not tests.**
`ADR_0022:78` states "The current reader floor is schema 34" while `LATEST_SCHEMA_VERSION` is
40 — and it does not mention the schema 36, 37, 39 or 40 floors at all. That is genuine drift,
and it is a comment.

**Conclusion: delete nothing from the test tree.** If the goal is removing tech debt, the debt
is the eleven structural defects in §1, not the suite that guards them. A suite that tests the
migration chain from schema 1 is precisely the asset that makes the extractions in §5 safe to
attempt.

---

**Delivered, in order.** Skeleton (0) · `wns-kernel` (1) · `wns-storage` (2) ·
`wns-documents` (3) · `wns-providers` (4) · `projects.rs` split (D3 prerequisite) ·
the five per-concern facades — `ProjectSession` from 28 public methods to 8 (5) ·
the `context/` move and the vocabulary inversion it needed (6) · the shared primitive
layer to L0/L1 and the first two of the twenty-one `projects/` modules (7, in progress) ·
frontend `kernel/` (§4.2) · frontend save loop (§4.3). Plus two defects fixed: the
`ADR_0022` reader-floor drift, and a correction to this document's own test-tree audit.

**Not yet done, and the next three steps in dependency order.**

1. **`context/packet.rs` (step 6).** Still 3,462 lines, still the file §3.4 describes as the
   hardest. Two of its three blockers are now cleared:
   
   - The error and identity types (`CoreError`, `CoreResult`, `Head`) had already moved to the
     kernel; the fifteen sites reaching up for them now name the kernel directly.
   - **`Revision` moved to `wns-kernel` and `story_records` moved to `wns-context`.** The record
     shapes a compiled packet carries are the compiler's *input* vocabulary, so leaving them in
     the crate under decomposition is exactly what made the compiler reach upward for its own
     inputs. Both are re-exported at their historical paths, so no call site changed.
   
   What remains is `story_context` (12 refs), `reviewed_summary` (4) and one each for
   `workshop_generation`, `project_chat_output` and `project_chat_context`.
   
   **And those are genuine inversions, not import tidying.** §3.4's "the compiler should
   receive already-materialised inputs rather than assemble them" is the fix for all three of
   these, which the design did not know:
   
   - `reviewed_summary.rs` imports `crate::context::SourceRef`. It is not vocabulary beneath
     context — it is a *consumer* of it, so moving it down creates the cycle in the other
     direction.
   - `story_records.rs` imports `Revision` from `projects/records.rs`, so the record vocabulary
     is not self-contained.
   - `FrozenContext`, `SourceRead`, `SourcePassage` and `SearchResult` have no `impl` blocks in
     `story_context.rs`. The types can be lifted; the operations on them cannot be, at least
     not in the same step.
   
   That is why this step is not a file move. The packet-compiler inversion resolves all three at
   once, and is the right unit of work.

   **Measured, then confirmed by attempting it.** An attempt to move the vocabulary down one
   module at a time was abandoned, and the failure is the finding. `contracts.rs` looks like the
   perfect first move — 341 lines, 22 types, and a grep for `crate::projects` returns zero. But
   it references `crate::context::{lookup, navigation, reviewed_evidence, reviewed_knowledge,
   reviewed_promises, reviewed_summaries}` — six *sibling* paths, which a `crate::projects` grep
   cannot see. In turn `lookup` and `navigation` import `crate::projects::story_context`, the
   very module the move was meant to empty. The closure is the whole of `context/`, including
   `packet.rs`.

   So step 6 is genuinely atomic: `contracts` → six siblings → `story_context` → back to
   `projects`. There is no first module that can move alone, and the attempt cost one revert.
   The lesson generalises past this file — **grepping for upward references finds the ones that
   cross a crate boundary and misses the ones that cross a module boundary inside the same
   crate**, which is exactly the shape of a rename in progress.

   **The whole move was then attempted in one change, and it reached one error.** All 18 context
   modules (9,166 lines) moved, `frozen.rs` was carved out of `story_context.rs` with
   `search_saved_passages` and its `literal_spans` helper, `ReviewPrefixItem` and
   `reviewed_summary` came down, `ProjectAccess` went to the kernel, and the crate compiled down
   to a single unresolved import. That import is the one thing left:

   **`packet.rs` needs `workshop_generation::{WORKSHOP_RESPONSE_CONTRACT,
   metadata_from_instruction, metadata_value}`.** The two functions round-trip a
   `WorkshopPacketMetadata`, which pulls `WorkshopExploration`, `WorkshopLiteral`, `Lens` and
   `WorkshopDepth` from two different files — **and `WorkshopPacketMetadata` has an inherent
   `impl` block**, which by Rust's rules cannot live in a different crate from its type. So the
   cluster has to travel with its behaviour, not just its fields.

   The attempt was reverted after that, cleanly, because finishing it means deciding whether the
   workshop packet-metadata vocabulary belongs at L2 with the compiler or whether the compiler
   should *receive* the parsed metadata instead of parsing it itself.

   **Decided and done: the compiler receives it.** `impl WorkshopPacketMetadata` is not a
   record — `from_context` validates against `WorkshopContext`, `validate_exploration`,
   `validate_text` and `validate_id`, which is workshop *domain* logic, so moving the type down
   would have dragged a validation cluster with it. Instead `PacketRequest` gained
   `workshop_metadata: Option<Value>`, `context_packets.rs` — the one place a workshop
   instruction is authored, and therefore the only code that can prove one valid — parses and
   validates it and passes the value down, and `packet.rs` consumes it. The compiler's
   `metadata_from_instruction` call became a presence check, and its second call, which
   discarded its result and existed only as validation, is gone.

   That is the §3.4 principle applied literally: *the compiler should receive materialised
   inputs rather than assemble them*. It also removed the last thing `packet.rs` reached up into
   `projects` for, leaving `context/` free to move.

   **There are three builders, not one, and the tests found the other two.** Making the compiler
   *receive* an input moves a burden onto every caller, and the first pass supplied it at one
   site. Eleven workshop tests failed with *"The stored request cannot reproduce its packet"*,
   which located the rest precisely:

   - `discussions.rs` builds its own `PacketRequest` for a workshop start; the generic
     `context_packets.rs` path is not the only entry.
   - `context_packets.rs`'s **reproduction check** recompiles a stored request to prove the
     stored packet still matches it. That path must supply exactly what the original compile was
     given, or it verifies a packet against a request that no longer matches — a silent
     integrity check that would have passed while checking the wrong thing.

   The second is the one worth remembering. A reproduction path is invisible to a
   type-driven change: the code compiles, the field has a value, and only a test that actually
   starts a workshop notices that the value is *wrong* rather than absent.

   **Step 6 done.** With that last dependency inverted, all eighteen context modules moved —
   9,166 lines into `wns-context`, which now owns `packet.rs`, the eligibility kernel, the
   contracts, and the vocabulary the inversion pulled down (`frozen`, `story_records`,
   `chat_vocabulary`, `response_contracts`, `reviewed_summary`, `reviewed_prefix`). `frozen.rs`
   was carved out of `story_context.rs` with `search_saved_passages` and `literal_spans`;
   `ProjectAccess` went to the kernel; `crates/core/src/context/` no longer exists and
   `webnovel-core` re-exports the crate as `context`, so not one call site changed.

   The whole thing is one commit because it could not be anything else — which is the finding
   two aborted attempts paid for.
2. **`wns-story`, `wns-conversation`, `wns-workshop` (step 7)** — **gated on a design decision,
   not on effort.** All twenty-one modules under `projects/` define an inherent impl on
   `ProjectSession` or `OwnedProject`:

   ```
   background_work · context_packets · discussions · discussions/app_server · evidence_queries
   exports · guidance · history · memory · memory/app_server · project_chat · project_chat/chapters
   project_chat/draft_lifecycle · project_chat/materialize · project_chat/store · proposals
   reviewed_story · session · source_pins · story_context · workshop
   ```

   In Rust an inherent impl must live in the crate that owns the type, so **no domain module can
   move to another crate while its methods are inherent impls on the actor's types.** And the
   actor cannot move down to meet them: `OwnedProject` dispatches twelve domains
   (`handle_context`, `handle_discussion`, `handle_memory`, `handle_workshop`… via 39 `Command`
   variants), so it sits above all of them by construction.

   This is the step-6 `impl WorkshopPacketMetadata` problem at the scale of the whole actor.
   Step 6 needed one type's behaviour to travel with it; step 7 needs thirty-five impl blocks
   across twenty-one modules to stop being inherent impls. The shapes that work:

   - a trait per concern that the actor implements, with the method bodies as free functions in
     the owning crate, or
   - the per-concern facades of §3.5 growing into the real interface, with the actor reached
     through a narrow host trait rather than named directly.

   Either is a design decision, and it is the remaining design question of this migration. It is
   also the last one: steps 8 and 9 are ordinary moves once the actor stops being a type every
   module can add methods to.

   **Decided: a host trait per concern.** The trait is declared in the target crate and exposes
   only what one module needs from the actor; `OwnedProject` implements it in core; the module's
   impl bodies become free functions taking `&mut impl TheHost`. Measured on the smallest
   candidate, `projects/memory/app_server.rs` (227 lines), the split is clean:

   - its `impl ProjectSession` block is two methods that each do nothing but
     `self.request(|reply| Command::Memory(...))`, so the session half needs `request` alone and
     could become a facade method directly;
   - its `impl OwnedProject` block needs `db`, `db_mut`, `check_access` **and** the
     crate-private helpers the bodies call — `validate_runtime_owner`, `read_memory_job_row`,
     `validate_job_owner`.

   That second half is the actual work, and it is worth stating plainly: **a host trait is
   narrow only if the helpers travel with the module.** A module whose bodies call six private
   functions from the crate they are leaving does not have a narrow host; it has a wide one with
   extra steps. The estimation for a real module is therefore *"how many crate-private free
   functions do its impl bodies call?"*, not how many actor methods they call — and that number
   is readable with one grep per module before committing to the pattern.

   **And the helpers are the hard part, measured rather than assumed.** Taking the same smallest
   module and following its four helpers to their definitions:

   | Helper | Defined in | Also used by | Shape |
   |---|---|---|---|
   | `invalid` | `conversation_context.rs` | 4 other modules | trivial |
   | `read_memory_job_row` | `memory.rs` | `memory.rs` | takes `&Connection` — movable |
   | `validate_job_owner` | `memory.rs` | `memory.rs` | takes rows — movable |
   | `validate_runtime_owner` | `discussions.rs` | **`discussions.rs` too** | **takes `&OwnedProject`** |

   The last one is the blocker and it is the pattern in miniature. A helper that takes the actor
   by name cannot move to a crate that does not own the actor — so it must first be generalised
   over the host (`&impl Host`) rather than over `OwnedProject`. It is also shared with
   `discussions`, which means generalising it is a step that two modules depend on, and doing it
   for one module pulls the other along.

   **So the first real action of step 7 is not moving a module.** It is generalising the shared
   helpers — `validate_runtime_owner` above all — over the host, in place, with everything still
   compiling. That is a bounded, verifiable change whose only purpose is to unblock the moves,
   and it is the correct first commit of the step rather than a precursor to it.

   **Done.** Both `validate_runtime_owner` definitions — one in `discussions.rs` for `RunOwner`,
   one in `memory.rs` for `MemoryOwner` — read exactly two fields off the actor
   (`project.info.project_id` and `project.info.operation_namespace`) and nothing else. So
   neither needed the actor at all: they now take `&ProjectInfo`, and their twenty-two call
   sites pass `&self.info`. Ten lines of signature change, no behaviour touched, and the shared
   helper that blocked two modules is now movable.

   That is what the pattern costs when the measurement is done first. The doc predicted this
   would be "a bounded, verifiable change" and it was — but only after four failed estimates
   taught me to read the helper's body before deciding whether it could travel.

   **And the host trait is genuinely narrow.** With the helpers generalised, the whole of what
   `memory/app_server.rs` needs from the actor is:

   ```
   self.info     ×2   (for the generalised validate_runtime_owner)
   db_mut()      ×2
   ```

   Two methods. Not six, and not the twelve domains the actor dispatches. The earlier count of
   "self.x()" calls was misleading because chained calls (`self\n    .db_mut()`) do not match a
   same-line grep — a measurement error of exactly the kind this migration keeps producing, and
   caught the same way: by reading the file rather than the pattern.

   So `MemoryHost` is `{ fn info(&self) -> &ProjectInfo; fn db_mut(&mut self) -> CoreResult<&mut Connection>; }`,
   the `impl OwnedProject` bodies become free functions taking `&mut impl MemoryHost`, and the
   `impl ProjectSession` half stays in `webnovel-core` because it constructs `Command` values the
   actor owns. That is the intended split: command plumbing with the actor, domain logic in the
   crate, and the trait as the only thing between them.

   **But the host trait is not the unit, and that is the last measurement.** The `impl
   ProjectSession` half cannot stay behind the way the split above assumes: it does
   `Command::Memory(Box::new(MemoryCommand::ClaimAppServer(owner, dispatch, reply)))`, so
   `MemoryCommand` and everything in its twelve variants must be reachable from wherever those
   methods live. `MemoryCommand` carries `MemoryOwner`, `AppServerDispatch`, `StartMemory`,
   `MemoryJob`, `MemoryDispatch`, `CompleteMemory`, `MemoryCompletion`, `MemoryView`,
   `MemoryRead` and `MemoryList` — roughly ten types, all defined in `memory.rs`.

   Core's `Command` enum is the only other consumer, and it sits *above* the new crate, so
   referencing `wns_story::MemoryCommand` from there is a legal downward edge. The move is
   therefore possible — **but its unit is the whole memory module's vocabulary, not the
   `app_server.rs` file.** A host trait narrows what a module needs from the *actor*; it does
   nothing about what the module's own command enum needs from the *actor's command enum*, and
   that second dependency is what actually sets the size.

   So the sizing question has a third form, after "how many actor methods" and "how many private
   helpers": **"does this module's `Command` variant own a vocabulary that other modules share?"**
   For memory the answer is ten types. That is readable before starting, by looking at the enum's
   variants rather than at the file's imports — and it is the number that decides whether a
   module is a step-7 unit or a step-7 project.

   **And every module has this shape, which is the real conclusion about step 7.** The pattern is
   identical in `discussions`, `evidence_queries`, `exports`, `source_pins`, `background_work`,
   `guidance`, `history`, `proposals`, `reviewed_story`, `workshop` and the rest: each file holds
   an `impl ProjectSession` that constructs its own `Command` variant, an `impl OwnedProject`
   whose bodies do the work, and the vocabulary those two share. The session half must stay with
   the `Command` enum in `webnovel-core`; the other two travel. So **the unit of step 7 is one
   module's vocabulary plus its actor-side logic, and the session half is a permanent resident of
   `webnovel-core` until the `Command` enum itself is replaced.**

   Measured on `memory.rs` (2,436 lines), the largest of the group: the actor-side impl calls
   `check_access`, `db_mut`, `fence_uncertain` on the actor, and outside itself reaches
   `compile_packet` (already in `wns-context`), `freeze_memory_story_at` and `read_source`
   (in `story_context`, which moves with it), `persist_compiled_packet_at` (in
   `context_packets`), plus `new_id` and `sha256_hex` (kernel). A host of six or seven methods,
   and a vocabulary of ten types.

   That is a session's work per two or three modules, not a turn's — and it is the last thing
   this document can usefully measure. What remains is execution against a pattern that is now
   fully characterised rather than partly guessed.

   **And the pattern is now proven, not just described.** `source_pins.rs` was the first module
   to move — chosen because its command vocabulary is the smallest in the tree at two arms.
   `wnsd-story` owns the vocabulary (`SourcePinScope`, `SourcePinSet`, `SourcePinsView`,
   `SaveSourcePins`, `SourcePinCommand`), the actor-side logic, and every helper in the file;
   `webnovel-core` keeps the two `impl ProjectSession` methods, because they construct
   `Command::SourcePins` and an inherent impl must live with its type.

   Between them is `SourcePinHost` — **four methods**: `check_access`, `db`, `db_mut`,
   `fence_uncertain`. `OwnedProject` implements it in core in twelve lines, and
   `handle_source_pins` becomes a one-line delegation. Zero errors, zero warnings, and the
   suite unchanged.

   Three things had to travel down first, and each was found by the compiler rather than by
   planning: `Reply` (every module's command enum names it), and `parse_version` /
   `parse_stored_version` / `logical_hash` (eleven to twenty callers each). That is the ordering
   lesson of step 7 — **the shared vocabulary moves before the modules do**, or each module move
   re-discovers the same blocked helper.

   **And order between the modules themselves matters too.** The next candidate,
   `evidence_queries.rs` (516 lines, seven command arms), was attempted and reverted: its actor
   side calls `reviewed_story::current_records_for_sources`, takes a `ReviewedRecordSet`, and
   calls `story_context::load_snapshot`. All three are still in `webnovel-core`, and none is a
   helper — they are domain modules that have to move first.

   So `source_pins` succeeded because it was self-contained: every helper it used lived in its
   own file. That is the selection criterion for the next module, and it is checkable before
   starting — **does the module's actor side call into other modules under `projects/`?** If it
   does, those modules are its predecessors in the ordering, and extracting it first fails at
   the end rather than the beginning.

   **That criterion is incomplete, and the way it failed is the finding.** Reading the whole of
   `projects/` for cross-module calls — the count of `other_module::` references per file —
   turns up exactly two modules that call nobody: `history` and `reviewed_story`. So `history`
   was attempted next, and it was not self-contained either. Its actor side reached **eight
   crate-private free functions that live in `projects.rs`**:

   ```
   read_document · read_document_with_role · read_revision · checkpoint_at
   existing_receipt · insert_receipt · valid_hash · require_head
   ```

   Those eight had **240 call sites across the twelve files under `projects/`** — every
   remaining module's actor side calls the same ones. A grep for cross-module calls cannot see
   them, because they are not calls *into a module*; they are calls into the file every module
   already sits inside. That is why the criterion said "clean" about a module that was not.

   **So the real gate on step 7 was never the host trait. It was the primitive layer.** The doc
   above says "the shared vocabulary moves before the modules do" and reads that as being about
   types — `Reply`, `parse_version`. It is about helpers too, and the helpers were the bigger
   gate: while eight functions with 240 call sites lived above the schema they read, no module
   could travel, no matter how narrow its host trait.

   **Done, in one commit.** The cut is by layer rather than by module:

   | To | What | Why there |
   |---|---|---|
   | L0 `wns-kernel` | `DocumentRecord`, `DocumentRole`, `StoredResult`, `RestoredDecision`, `AppliedDecision`, `new_id`, `valid_hash`, `require_head` | vocabulary — the records, and the two leaf types `StoredResult` names |
   | L1 `wns-storage` | `read_document`, `read_document_with_role`, `read_revision`, `checkpoint_at`, `existing_receipt`, `insert_receipt` | they query the tables this crate's migrations create |

   `StoredResult` naming `history::RestoredDecision` and `proposals::AppliedDecision` looked like
   a cascade back into two modules, which is what it was: the receipt type could not reach L0
   until its two leaf types did. Both are strings-only structs, so the cascade stopped there.
   `DocumentRole::storage_name` and `::from_storage` became `pub` because the row readers now
   decode with them across a crate boundary. All eleven items are re-exported at their historical
   paths, so all 240 call sites are unchanged.

   **And `history` moved next, which is what proves the point.** `wnsd-documents` owns the
   vocabulary and the actor-side logic as free functions over a four-method `HistoryHost`;
   `webnovel-core` keeps the three `impl ProjectSession` methods, because `Command::History` is
   a variant of the actor's own enum. The move was mechanical — once the primitives were below
   it.

   Two things surfaced in that second move that the first did not:

   - **`history` had a coupling no import check finds: a test hook.** Its restore path calls
     `super::tests::hold_after_commit_before_ack`, the crash-injection point for
     `kill_after_commit_before_ack_recovers_once`. Two modules use it and both are moving.
     The resolution is a `HistoryHost` method declared unconditionally — so the trait compiles
     in a non-test build of the crate — whose implementation in core compiles the real hook only
     into core's test binary. Production behaviour is untouched and the hook does not become
     reachable from a released library.
   - **The layering guard caught a second-order effect rather than letting it through.** Document
     history reads and writes document rows, so `wns-documents` must depend on `wns-storage` and
     rises from L1 to L2. But `wns-context` already depended on `wns-documents`, so the move
     produced a sideways L2→L2 edge — and the test failed with exactly that, by name. Every layer
     above documents shifted up one ordinal: context 3, story 4, conversation/workshop 5. The
     **relative** order is unchanged; these are ordinals in a partial order, and no existing edge
     changed direction. That is the layering test earning its keep on a change that was not
     about layering at all.

   **`reviewed_story` moved third, and it is the one that proves the ordering matters.** At 3,133
   lines it is the largest module to move so far, and the first that other modules call *into* —
   `evidence_queries`, `exports`, `story_context` and `transfer` all reach for its functions,
   which is exactly why it precedes them. It went to `wns-story`, its namesake crate, and needed
   no new graph edges at all: its only outside dependencies were `wns-context` types, already
   below it.

   It was also the cleanest move of the three, and the reason is the ordering rather than the
   module. It has no test hook, no sibling `projects/` call, and its actor side reaches the same
   eight helpers `history` did — all of which were already at L0 and L1 by the time it moved.
   `ReviewedStoryHost` is four methods, with no crash hook, because four methods is all it needs.

   Two things still had to be found rather than planned, both by the compiler:

   - **A ninth shared helper, `validate_title`,** nine callers. Same class as `valid_hash`: a
     pure predicate with no reason to live above the schema, surfaced only because a module that
     used it left.
   - **The internal cross-calls.** The module's own methods call each other, and once the
     receivers became host parameters those calls had to become free-function calls —
     `host.chapter_review_status(…)` → `chapter_review_status(host, …)`, seven sites. A
     mechanical rewrite that moves `self.` to `host.` will not find them, because they are the
     right shape for the *old* code and the wrong shape for the new.

   And once more the measurement trap: two `self\n.db_mut()` calls, chained across a newline,
   escaped the rewrite. That is the fourth time this migration has hit that exact pattern.

   The comfortable reading of this step is that it is twenty-one identical moves. It is not, and
   the corrected reading is sharper than the one it replaces: **a module can move when everything
   its actor side reaches is already below it** — the domain modules it calls, the crate-private
   helpers its bodies use, and the test hooks its crash paths reach. None of the three is visible
   in the imports, which is why the only way to find the order is to try.

   Five of the twenty-one have moved: `source_pins`, `history`, `reviewed_story`,
   `project_chat_output`, `material_adoption` — chosen in that order not for size but for
   reachability, and the later ones' ease is the return on the primitive-layer commit that
   made them possible.

   **And there is a fourth category of module, which the three before it hid.**
   `project_chat_output` (1,113 lines) has **no `impl` blocks at all**: no `ProjectSession`
   half, no actor-side half, no command vocabulary. The answer to "what does this need from
   the actor?" is *nothing*, so it needed no host trait and no `Command` variant — the entire
   move was two import rewrites and a glob re-export. It is what step 7 is *supposed* to look
   like, and it appeared only after three modules taught the pattern that made it findable.
   Its 11 tests moved with it, so the workspace total is conserved exactly.

   That reframes the remaining sixteen. Some are pure vocabulary like this one; some are
   `history`-shaped, needing four host methods; some carry a command enum of their own. The
   work is not uniform, and the doc can now name which kind a given module is *before* it is
   attempted, which is the whole of what four modules bought.

   **`material_adoption` moved fifth, and it is the one that shows a fifth constraint.**
   It is pure like `project_chat_output` — no `impl` blocks, no host trait — but it has two
   consumers, `project_chat/adoption.rs` and `workshop.rs`, and those are **siblings at L5**.
   A helper that two siblings both need cannot live in either of them, so it went to
   `wns-documents` (L2): below both, and the right home on the merits, since it takes a
   caller-owned transaction and writes a document body with a before/after checkpoint pair.

   That is a rule the first four moves could not have revealed, because all four had exactly
   one consumer or none:

   > A shared helper's layer is set by its **lowest** consumer, not by the module it was
   > extracted from.

   The fifth move also carried `blank_document` down with it — one paragraph with a fresh id,
   which had sat beside the actor for no reason except that it was written there first.

   **Surveying the rest with the criterion now costs one command.** For each remaining module:
   does it have a `ProjectSession` half, an actor-side half, a test hook, and which siblings
   does it call? That table is what picked `material_adoption` and what rules out the rest —
   `exports` is blocked on step 8 (`crate::transfer` is still in core), `import` likewise, and
   everything else waits on `story_context` or `discussions`.

   **Then the `story_context` group, and the cycle inside it.** `story_context` is the next
   bottleneck — `guidance`, `evidence_queries`, `context_packets`, `discussion_lookup`, `memory`
   and `workshop` all wait on it — but it and `guidance` call each other: `story_context` pins
   guidance into every snapshot it freezes, and `guidance`'s `retry_request_guidance_at` takes a
   `story_context::FrozenContext`. Under the ordering rule that means they move *together*, into
   whichever crate can hold both.

   They cannot. `story_context` is `wns-story` (L4) and guidance authoring is `wns-conversation`
   (L5), and the call direction differs by half: `story_context` → guidance-selection is an
   L4→L5 *upward* call and illegal, while `guidance` → `FrozenContext` is L5→L4 and fine.

   **So the fix is not to move both. It is to move one half of the cycle down** — and the halving
   is already there in the file, because `wns_context::guidance` has held `FrozenGuidance`,
   `GuidanceVersion` and `GuidanceScope` since step 6. The *frozen* half — row conversion, the
   reads, and `select`/`pin`/`validate` at a snapshot — moved to L3. The *authoring* half — heads,
   immutable versions, receipts, and the commands that append them — stays in core, bound for
   `wns-conversation`.

   Eleven items moved, and the authoring half still reaches down for five of them
   (`GuidanceHead`, `parse_scope`, `head_to_version`, `valid_guidance_hash`, `validate_text`),
   which is a legal downward edge from either core or L5. The upward call is gone and the
   downward one is unchanged.

   The general lesson is worth stating, because it is the first time this migration has needed
   it:

   > When two modules are mutually dependent but belong at different layers, move the **half
   > that the higher-layer module calls** down to the seam between them — not both modules to
   > one crate. A cycle is an ordering constraint, and it can be discharged from either end.

   Moving both would have put guidance in a crate whose stated concern is the conversation that
   *authors* it, which is a worse boundary than the one this produced.

   **And the same trick applies twice more, which shrinks the group a lot.** The 6,363-line
   figure above counted the modules, not the edges. Measured, `story_context`'s remaining
   dependencies on that group are two calls into `conversation_context` and **one** call into
   `memory`:

   | Edge | What crosses | Mirrors |
   |---|---|---|
   | `story_context` → `conversation_context` | `select_conversation_at`, `validate_conversation_at` | exactly the `guidance` shape — a frozen-selection half |
   | `story_context` → `memory` | `validate_navigation_view_record` | a single function |

   Both are the *caller's* half of a mutual dependency, so both discharge the same way guidance
   did: move them down to the seam, not the modules. `select_conversation_at` and
   `validate_conversation_at` are the frozen half of the conversation the same way
   `select_guidance_at` is the frozen half of guidance, and `validate_navigation_view_record`
   belongs with navigation in `wns-context` regardless.

   That leaves `project_chat_context`, whose dependency on `story_context` needs nothing at all:
   `FrozenContext`, `FreezeStory` and `freeze_project_chat_at` are L5 calling L4, which is legal
   the moment `story_context` is at L4.

   So `story_context` is three small moves away from being movable on its own — and it is the
   largest single unblock left, since `guidance`, `evidence_queries`, `context_packets`,
   `discussion_lookup`, `memory` and `workshop` all wait behind it. The estimate that mattered
   was never the line count of the group; it was the number of edges crossing out of it, and
   that number is three.

   **Two of the three are done, and the third is not what it looked like.** The
   `conversation_context` half went the same way as guidance — and more cheaply, because
   `decode_snapshot` belongs beside the type it constructs and the selectors beside the
   `FrozenConversation` they produce, so both landed in `wns-context` without a host trait
   (`conversation_context` has no `impl` blocks either). `decode_snapshot`'s two test-only
   helpers travelled with it: `eligibility`, a one-line wrapper over `evaluate_sources`, which
   was already there.

   But the "**one** call into `memory`" was measured by call *sites*, and that is the same unit
   error this document has now made three times. `story_context` calls
   `validate_navigation_view_record` — one function, 65 lines — and that function's body reaches
   `MemoryView`, `MemoryJobRow`, `read_memory_job_row`, `validate_memory_job_record` and
   `read_memory_view`, each with its own closure, two of them shared with `memory/app_server.rs`.
   It is a slice of `memory.rs`, not a function.

   So the count that matters is not edges either, but **the transitive closure of what crosses
   one**. Every estimate in this step has been one level too shallow: modules → helpers → helper
   closures. `story_context` and `memory` are also both bound for `wns-story`, so once `memory`'s
   own blockers clear, moving them *together* discharges this cycle for free and the slice never
   has to be carved out at all. That is the likely resolution, and it is why this one was left
   rather than forced.

   `memory` reaches `context_packets` and `discussions`, which are conversation-side and do not
   shrink this way. That is the remaining knot.

   **And the technique has a boundary, which the next measurement found.** The obvious move for
   the rest of the cluster is the one that worked twice already: `story_context`'s operations are
   a shared surface by the same measurement — `validated_snapshot_record` at **17 call sites in
   10 files**, `load_snapshot` at 12, `read_source` at 10 — so move *those* to L3 beside
   `decode_snapshot`, exactly as `projects.rs`'s primitive layer moved to L0/L1.

   It cannot be done, and one line says why:

   ```
   validated_snapshot_record → validate_pins → reviewed_story::ReviewValidationContext
   ```

   `reviewed_story` is at L4. A shared surface that depends on an L4 type cannot sit at L3, and
   `validate_pins` is not separable from the record — it is what makes a validated record
   *validated*.

   > Moving one half of a cycle down works when that half genuinely belongs at the lower layer.
   > It fails when the shared surface depends on types that belong at the higher one — and then
   > the dependency is not an ordering accident to be discharged, it is the boundary asserting
   > itself.

   That is the difference between this cluster and the two that yielded. Guidance's frozen half
   and the conversation selectors were L3 concerns by their own nature: they produce
   `FrozenGuidance` and `FrozenConversation`, both already at L3. `story_context`'s operations
   produce a *validated* `FrozenContext`, and validation is the story boundary.

   So the resolution for this cluster is the one the first four modules used: move the modules,
   not the surface. `story_context`, `memory` and `context_packets` are all bound for
   `wns-story` and are mutually dependent, which makes them one unit — and their two escapes
   (`story_context → project_chat_context` via `validate_frozen_project_chat`, `memory →
   discussions`) are both L4→L5 upward edges that each need their own discharge first.

   That unit is ~5,000 lines across three files, it is the last one in step 7, and it is the
   session's work this document has now predicted four times and measured properly once.

   `wns-library` (step 8) is still additionally blocked on `projects::import` being a direct
   module import; `crates/architecture` will refuse the backward edge if it is attempted too
   early.

On the frontend, §4.1 (feature slices), §4.4 (generated IPC) and §4.5 (shell reduction) remain.
§4.4 is the largest remaining correctness win: D6 — 255 hand-written type mirrors across 22
files with one tested — is untouched.

**One thing this branch does not claim.** The `SourceEpoch` type named in §2 does not exist yet
— it is a target for the invariant work, not a delivered type, and the current fencing is still
per-call-site.
