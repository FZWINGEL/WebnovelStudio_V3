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
                      ────────────────────────────────────────────────────────
L4  wns-conversation  discussions · project chat · proposals · guidance · adoption
    wns-workshop      six-lens Workshop, typed candidates, generation
                      ────────────────────────────────────────────────────────
L3  wns-story         memory · reviewed story · story context · evidence/promise history
                      · accepted summaries · character knowledge
                      ────────────────────────────────────────────────────────
L2  wns-context       packet compiler · eligibility · lookup · receipts · inspector
                      ────────────────────────────────────────────────────────
L1  wns-storage       SQLite · the 40 migrations · durability · backups-before-upgrade
    wns-documents     snapshot validation · scope · structured blocks · editor contract
    wns-providers     Provider port + codex-exec / app-server / claude / http / mock
                      ────────────────────────────────────────────────────────
L0  wns-kernel        CoreError/CoreResult · sha256 · canonical JSON · W0 snapshot validator
```

**The arrow rule:** a crate may depend only on strictly lower layers. `wns-app` depends on
everything; nothing depends on `wns-app`. Siblings at one layer must not depend on each
other — `wns-storage`, `wns-documents` and `wns-providers` are mutually independent.

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
frontend `kernel/` (§4.2) · frontend save loop (§4.3). Plus two defects fixed: the
`ADR_0022` reader-floor drift, and a correction to this document's own test-tree audit.

**Not yet done, and the next three steps in dependency order.**

1. **`context/packet.rs` (step 6).** Still 3,462 lines, still the file §3.4 describes as the
   hardest. Its `projects::{project_chat_output, story_context, workshop_generation}` imports
   are the inversion to perform.
2. **`wns-story`, `wns-conversation`, `wns-workshop` (step 7)**, then `wns-transfer` and
   `wns-library` (step 8), then `wns-app` (step 9). `wns-library` was blocked on step 5 and no
   longer is — `library.rs` now reaches `ProjectSession::documents()` and `project()` instead
   of the flat 28-method surface — but `projects::import` is still a direct module import and
   must be inverted before it can move. `crates/architecture` will refuse the backward edge if
   it is attempted too early.

On the frontend, §4.1 (feature slices), §4.4 (generated IPC) and §4.5 (shell reduction) remain.
§4.4 is the largest remaining correctness win: D6 — 255 hand-written type mirrors across 22
files with one tested — is untouched.

**One thing this branch does not claim.** The `SourceEpoch` type named in §2 does not exist yet
— it is a target for the invariant work, not a delivered type, and the current fencing is still
per-call-site.
