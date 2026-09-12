# CLAUDE.md

# WebnovelStudio V3

- V3 is the independent Rust/Tauri rewrite. V2 at `D:\WebnovelStudio_V2` is read-only reference material for V3 work; do not edit V2 or its author data.
- Read [PRODUCT.md](PRODUCT.md), [docs/README.md](docs/README.md), and the relevant architecture/plan before changing behavior. Source and executed checks establish current status; planned checkboxes do not.
- The user has authorized completing V3 beyond W0. Follow the dependency order in the first-slice plan; W1/W2 persistence and reconciliation precede a persistent writing UI. The W0 trial remains session-only until that path is integrated and verified. Keep current evidence and remaining work in `docs/IMPLEMENTATION_STATUS.md`.
- The product targets English authoring, UI, and export. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; do not add Chinese authoring or IME acceptance requirements.
- Codex is the primary provider focus. Keep the Claude and OpenAI-compatible integrations; further adapter ports are deferred at the author's request. New summary and story-memory model calls use GPT-6 Astra/low independently of the writing-model picker, per the author's 8 September instruction. Preserve historical Luna/xhigh bindings and receipts without rewriting them.
- JavaScript owns live Tiptap/ProseMirror transactions. Rust independently validates the restricted snapshot contract over IPC. Do not add a general ProseMirror-step interpreter or a second editor engine.
- Preserve the later lifecycle and restore decisions: explicit project/document/session identity, conservative stale-edit refusal, short local Apply barriers, and restore into a new recovered project. These become durable implementation work after W0.
- Keep toolchain and package pins in the root manifests. Use synthetic fixtures and temporary paths; never commit author databases, credentials, generated native results, or backups.
- Proposed commands are not evidence. Record native WebView2 text input, accessibility, persistence, provider, and packaging evidence in the qualification documents before claiming support.
- Register new core integration test files in `crates/core/tests/integration.rs`; see [development checks](docs/TESTING.md) for focused commands and the full verification path.

## Instruction Synchronization

**All agent instruction files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, and `.claude/CLAUDE.md`) must always remain synchronized.**
Whenever guidelines, architectural rules, tooling requirements, or workflow constraints are added, modified, or removed in any instruction file, the changes must immediately be propagated across all equivalent files to guarantee identical standards across all AI agents and harnesses.

## Preferred Tooling: CodeGraph & Context Mode

Always prefer and prioritize using **CodeGraph** and **Context Mode** over raw tool calls:

1. **CodeGraph (Primary Code Intelligence)**:
   - In repositories indexed by CodeGraph (where `.codegraph/` exists), reach for CodeGraph **BEFORE** using `grep`, `find`, or reading raw source files.
   - Use `codegraph_explore` (MCP) or `codegraph explore "<query>"` / `codegraph query "<symbol>"` (shell) to understand symbols, call paths, and source code.
2. **Context Mode (Primary Execution & Sandbox)**:
   - Always prefer `context-mode` MCP tools (`ctx_execute`, `ctx_batch_execute`, `ctx_execute_file`, `ctx_search`, `ctx_index`, `ctx_fetch_and_index`) to preserve context window capacity and session continuity.
   - **Think in Code**: When analyzing, counting, filtering, comparing, searching, or transforming code and data, write a script via `ctx_execute` (or `ctx_execute_file`) that computes and outputs only the concise answer (`console.log`), rather than reading large files or dumps into context.
   - **Avoid Raw Dumps**: Do not dump large command outputs (>20 lines), large web pages, or multi-file contents into the prompt. Use `ctx_fetch_and_index` + `ctx_search` for web documentation and `ctx_batch_execute` for batch commands.

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->


## pm conductor — known tooling bug (recorded 11 September 2026)

`add-many` **silently discards** a batch entry's `links` key. Every spelling fails quietly — no
error, no warning, and no `integrity` finding:

| Passed in the batch entry | Stored | Effect |
|---|---|---|
| `"links": ["depends-on:<epic>"]` | `[]` | dropped |
| `"links": "depends-on:<epic>"` | `[]` | dropped |
| `"links": [{"type":"depends-on","target":"<epic>"}]` | `{"type","target"}` | stored but inert — every engine reader keys on `epic`, not `target` |

Only `update-epic <id> --link "<type>:<epic>[:<reason>]"` records a working link.

**Rule: never pass `links` to `add-many`. Create the epics first, then add every relationship
with `update-epic --link`.** After a batch registration `integrity` is not sufficient — it
reports 0 findings on an inert link. Read the **Links** column in `PROJECT.md`: a link showing
`-` does not exist.

Full report: [.conductor/feedback/2026-09-11-add-many-silently-drops-links.md](.conductor/feedback/2026-09-11-add-many-silently-drops-links.md)


## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.



<!-- BEGIN pm-conductor rules (managed by pm — safe to delete this block) -->
## PM Conductor — operating rules

This repo is managed by the `pm` plugin. The conductor sits ABOVE OpenSpec and Superpowers.
Epics are **lane-agnostic** (openspec | superpowers | claude-code | decision | external);
OpenSpec is one lane. Stories come from each epic's source (OpenSpec `tasks.md`, a Superpowers
plan, or a manual list). Follow these rules:

1. **Detours** — when something blocks the active epic, CLASSIFY before fixing:
   - *Minimal* (small, self-contained, no design ambiguity): fix → test → commit → push,
     then run `/pm:detour --minimal "<what>"` so it is recorded in `.conductor/detours.log`.
     Then resume.
   - *Substantial* (own design / changes shared behavior / multi-step): run `/pm:detour`.
     It becomes its own epic in the appropriate lane (OpenSpec proposal, Superpowers plan,
     etc.). Register that epic FIRST, then PUSH the current one onto the detour stack with
     `push-detour <parent> --detour <new-id> --reason "<why>" (--reconcile | --no-reconcile)`.
     NEVER hand-edit `.conductor/state.json` to push or pop a frame. The verb IS the
     transition, and it is what supplies the validation, the write-conflict guard, the
     read-back verification and the Honcho line a hand-edit has none of. Exactly one of the
     two reconcile flags is REQUIRED and there is no default: whether the detour can
     invalidate the paused epic's plan is a judgment, and a default makes an absent decision
     look like a considered one. Say `--reconcile` unless you are certain the detour touches
     nothing the paused epic depends on.
2. **State of record is `.conductor/state.json`.** After any change to epics, status,
   priority, or the detour stack, re-render with `/pm:status`. Never hand-edit `PROJECT.md`.
3. **Resuming after a detour** — use `/pm:resume`, which pops the frame with
   `pop-detour [<paused-id>]` — again a verb, never a hand-edit. It removes the frame,
   resumes the epic and writes `reconcileNeeded` in the SAME write, which is what makes the
   obligation survive the frame's removal. If the popped frame had
   `reconcileOnResume`, run the reconcile gate (reconciler agent) BEFORE writing code,
   then write its verdict back durably with `record-reconcile <id> --detour <id>
   --verdict valid|invalidated [--amendments "<a>;<b>"]` — this attaches
   `{verdict, amendments, reconciledAt}` to the paused epic's link to the detour and
   clears `reconcileNeeded`, instead of the judgment only ever living in conversation.
4. **Honcho** — on every PUSH and POP, also write a one-line memory to Honcho
   ("paused X for Y" / "resumed X, reconciled vs Y") so the relationship survives outside
   this repo. `push-detour` prints the PUSH line for you and logs it to
   `.conductor/honcho-memories.log`; paste it into your Honcho tool call. `pop-detour` prints
   the POP line only when nothing needs reconciling — with a gate armed, "reconciled vs Y" is
   not yet true, so emit it with `honcho-memory pop <id> "<detour>; reconcile = …"` after the
   verdict. The engine formats and logs; it never calls Honcho itself.
5. **Keep `tasks.md` checkboxes truthful** — they are the source of truth for story progress.
6. **Roadmap as backlog** — work you intend to do but haven't proposed yet can be
   registered now with `/pm:epic add … --status planned` (any lane). Planned epics show
   as ordered backlog in `PROJECT.md` and a `planned: N` count in the briefing, without a
   "no change on disk" warning; `/pm:sync` flips an openspec planned epic to untriaged once
   its change is proposed. Have a roadmap doc? Read it in-session and load each item this way.
7. **Delegate discovery. If you do not already know the file path, do not go looking
   yourself.** A subagent's transcript never enters yours — only its final report does — so
   an open-ended read costs a conclusion instead of a transcript when it is delegated.
   "Where is X handled", "does a spec for this already exist", "what does this epic touch",
   "what did the last three changes here do": dispatch an `Explore` or `general-purpose`
   subagent and use what it concludes. Reserve an INLINE read for the narrow case where you
   already know the exact file and want one value out of it.
   This binds the ORCHESTRATING agent, which is the half that has no such rule: a dispatched
   child is already told to return a fixed report and not to narrate. It binds hardest across
   a hierarchy run or a multi-epic backlog, where your context survives many epics and is
   therefore the scarce resource — discovery you perform inline is paid for once per epic and
   never reclaimed.
   DELEGATING NEVER WEAKENS A FULL-READ REQUIREMENT. Where this instruction demands the whole
   document — the epic-level-autonomy preflight scan, and re-reading an epic's source before
   it becomes the work — the subagent reads the whole document and returns the finding. What
   is forbidden is substituting a keyword grep for a full read, and that is forbidden
   whoever performs it.

## Getting help with pm — two channels, and which one can lie

**The INSTALLED engine is the authority on what it accepts.** `node "$ENGINE" <verb> --help`
(resolve `$ENGINE` the way pm's own command docs do) prints that verb's real flags, projected
from its own registry — version-exact by construction. Use it before reading engine source.

**For procedure and rationale:** https://pm-plugin.dev/llms.txt indexes the docs (entries are
already markdown); a free, no-auth MCP at https://pm-plugin.dev/mcp answers in one call.

**The site documents the LATEST release, which may be newer than the pm running here.** A flag
it shows that your engine refuses is a version gap, not a bug — `/pm:changelog` says which.

## The gate procedure — required task items

Every item below is a NUMBERED REQUIRED TASK ITEM in the change's own task list, carried
into both gates. They are not review guidance and must not be restated as prose bullets:
measured across one audited repository, a rule carried by a mandatory task section reached
14/14 subsequent changes, while the same rule written as a prose bullet reached 3/15.

1. **Call-site completeness sweep.** For every rule, guard or invariant this change introduces
   or modifies, enumerate ALL call sites of the thing being guarded — derived mechanically
   (`rg` for the callers), never a list typed from memory, which goes stale the moment a
   caller is added. Then state where the rule holds and where it does not, and
   justify each omission. A guard added at one call site while an identical sibling site is
   left untouched is a FINDING, not a detail: raise it even though the unedited site never
   appears in the diff. Both gates are diff-scoped and structurally cannot see an edit that
   is absent from a file the diff never touched — the dominant defect class in this
   repository's own audit, ~38 instances in one shard.
   A DATA reference is a call site too: for every field the change adds that holds another
   record's id, enumerate the places that write it, read it and REMOVE it. A deletion path
   that strips one holder and not its siblings leaves a dangling reference — the record
   rendering a pointer to something that no longer exists — and it is invisible to both
   gates for the same diff-scoped reason.
   AN OPERATION HAS AN INVERSE, and the sweep above cannot reach it. For every operation
   this change adds or modifies, enumerate that inverse — set against unset, add against
   remove, append against replace, enable against disable, grant against revoke — then
   name and justify each inverse that is not shipped, exactly as an unguarded call site
   must be. An operation shipped without its inverse, and not justified, is a FINDING.
   The reason the sweep cannot reach this class is mechanical rather than a matter of
   diligence: enumerating the callers of a thing that is written never leads to the
   question of whether it can be unwritten. Measured here, six instances shipped past both
   gates while the call-site obligation was already in force, and the most consequential
   is a safety surface — pre-authorization grants accumulate with no revoke, so turning
   autonomy off leaves every prior grant intact and turning it back on silently restores
   all of them.
2. **Verify against the commit, not the working tree.** The commit is the unit of verification.
   Reading a file in the working tree is NOT verification. For every task, run
   `git show --stat <that task's sha>` and assert that
   every file the task claims to change appears in THAT commit. A task whose claimed file is
   absent from its commit FAILS, even though the working tree holds the intended edit, the
   suite passes and both gates are green. Audited here: two commits each claimed to remove a
   file's code and neither staged it, because a `git add` with an explicit path list aborted
   on an already-removed path — all four verification layers were reading the working tree,
   so nothing caught it, and it recurred after being written down in a commit message in the
   same epic.
3. **Declare lifecycle bookkeeping.** A task that is bookkeeping about the change's own
   lifecycle rather than its work — above all the task that ARCHIVES THE CHANGE ITSELF, which
   always qualifies — carries the literal marker `<!-- pm:lifecycle -->` ON THE TASK LINE.
   The engine infers this from nothing else: not the wording, not the commands the text
   names, not the position in the file. Mark it at the moment the task source is AUTHORED
   OR AMENDED — a source written before this capability existed gets the marker the first
   time you touch it, or its archive task counts as outstanding work forever.
   The marker is pm's alone and it COLLIDES with an upstream lint: `openspec validate
   --archived` knows nothing about it, counts raw checkboxes, and therefore FAILS every
   correctly archived pm change — reporting `1 incomplete task` against the same file pm
   reports complete with `· N lifecycle`. Its own help text offers it for pre-commit
   linting; do NOT wire it into a pm-managed repo. Nothing clears that failure: ticking the
   archive task would be a false record and dropping the marker would break pm's own archive
   gate. Ignoring a marked line upstream is the clean fix and it is not pm's to make.
4. **Attribute every commit to its epic.** At the moment each commit is made, record it:
   `update-epic <id> --attribute-commit <sha>`. The engine infers attribution from NOTHING —
   not the files a commit touches, not an epic id in a message — so an unrecorded commit is
   a commit the epic's Gate 2 cannot be checked against. The per-task conventional commit of
   an OpenSpec apply loop always qualifies. Work already in flight is covered too, but ONLY
   BEFORE the first attribution: catch up in the order the commits landed, then keep
   attributing forward. The array is append-only — the engine neither reorders nor
   de-duplicates it — so catching up AFTER attributing forward leaves an ancestor as the
   last entry, and the LAST entry is the endpoint a recorded Gate 2 `headSha` is compared
   against. If forward attribution has already begun, attribute forward only and say so;
   a wrong endpoint reads as a stale verdict and refuses the archive.
   ONE EXCLUSION, and it is not a judgment call: the commit that moves
   `openspec/changes/<id>/` under `archive/`, and any commit that only relocates or deletes a
   change's artifacts rather than implementing its work, is lifecycle bookkeeping and
   MUST NOT be attributed. That move lands after the reviewed range by construction, so
   attributing it
   makes the epic's own Gate 2 stale at the instant the archive gate reads it.
5. **Review a release's specs against each other.** Gate 1 and Gate 2 each take ONE CHANGE
   as their unit, so nothing above them asks whether a release's specs AGREE. Before
   `/opsx:apply` on any release holding two or more spec files — counted FLAT across its
   member changes, so one change carrying six specs qualifies — and again after any round
   of concurrent amendment, dispatch FRESH-CONTEXT reviewers at the release's whole spec
   set (one under `standard`, two with different lenses under `thorough`) and ask the six
   questions: contradiction, double ownership, unmeetable requirements, gaps against the
   proposal's Resolves list, vocabulary forks, and shared chokepoints. Split every finding
   into BLOCKS and POLISH, fix the BLOCKS, decline most POLISH and say why — a review of a
   large document always returns something, so "no findings" is not a stopping condition.
   A contradiction is never POLISH. Then record the verdict:
   `record-cross-spec-review <releaseId> --verdict pass|fail --reviewer "<identity>"`.
   The engine enumerates the spec set from disk and hashes it, so a spec ADDED to the
   release afterwards — or a reviewed spec amended — marks the verdict stale on every
   surface; a set you assert instead would go stale in exactly the way this gate exists to
   catch. Measured here: this pass returned 5 Critical and 10 Important against six specs
   that had each passed `openspec validate --strict` and would each have passed Gate 1
   alone, including a flagship scenario that was unreachable.
6. **End work by recording a disposition.** An epic, a story, a deferral or a release
   exclusion ENDS by recording a terminal disposition carrying its required reason, and
   never by removing the record. The archive verb takes TWO halves in ONE invocation — the
   disposition AND a deferral assertion — because the gate refuses either half alone:
   `update-epic <id> --status archived --outcome delivered|killed|superseded|abandoned|declined|unreconstructable --reason "<why>" --no-deferrals`
   (every outcome except `delivered` requires the reason). `--no-deferrals` is the explicit
   "there are none" and is a claim, not a default — swap it for `--deferral
   "<epicId>:<artifact section>"` where work is now held by a registered epic, or
   `--declined-deferral "<what>:<why not>"` where you are deliberately not doing it; both
   repeat, and the engine will not read your artifacts to guess.
   Deletion removes the record of projected work, which is
   precisely what a disposition exists to preserve. `remove-epic` stays available and
   ungated for what it is for: an epic registered in error, a duplicate, a mistake made a
   minute ago — where there is no disposition to record because there was no work.
7. **Route what the work taught you.** A change teaches three kinds of thing and each has a
   different destination. Route them BEFORE the change closes, while the evidence is still
   recoverable. Nothing above this asks, so silence here reads as "nothing was learned"
   rather than "nobody looked", and the two are indistinguishable afterwards.
   A PRACTICE, GATE OR DISCIPLINE you adopted to get this change done: register it as an
   epic, and file it with the tracker as well when it belongs to a product other people
   use. The evidence goes with it — what went wrong that made the practice necessary,
   with numbers. That evidence is the strongest part of the eventual spec and it is
   unrecoverable later; a practice registered without it reads as a preference.
   FRICTION IN THE TOOLING that you routed around: file it — `/pm:feedback [bug|feature]
   "<summary>"` for pm itself, and wherever it is tracked for anything else. THIS IS THE
   DIRECTION THAT GETS MISSED, and the reason is mechanical: a workaround produces working
   output, so nothing looks broken and nothing prompts. Hand-editing a file a tool owns
   because no verb exists for it, a command the tool EMITTED that did not run as written,
   a convention you invented that the tool should have supplied, anything you did twice by
   hand that it could have done once — each of those is a filing, not a footnote. Measured:
   two sessions hit one broken recipe in an afternoon, each invented a workaround, neither
   reported it until asked.
   A PROCESS FAILURE — how we work, rather than what the tool should do: a lesson file in
   `docs/lessons/`, carrying its `trigger` written as the situation BEFORE the mistake, a
   concrete `cost`, and `enforced_in` naming where its rule actually binds. Give it a
   `detect:` matcher only where the situation is recognisable with near-certainty — the
   `lesson-advice` hook fires on that matcher before the next mistake, and a hook that is
   wrong 7 times in 8 trains everyone to ignore the one time it is right, so a lesson that
   cannot be matched precisely stays retrieval-only.
   Name which of the three it is out loud. A process lesson filed as a feature request
   never gets built, and a product gap written down as a lesson never gets fixed.

## Intake — triage an ask against the whole backlog BEFORE registering it

The ask is the ONLY moment the whole backlog is cheap to consider: after registration nothing
ever re-reads it as a set, so an ask that duplicates existing work in another shape becomes a
permanent second epic. The dedup that already exists is IDENTITY-based — same id, or the same
`externalUrl` — which catches a re-run of sync and nothing else. Measured in this plugin's own
repository: four live pairs are one change registered twice under different lanes and
different names, and identity dedup found none of them.

1. **Get the candidate set mechanically.** Before any `add-epic`, run
   `/pm:triage "<the ask, in its own words>"`. It returns the existing epics that share
   distinctive vocabulary with the ask (each with the shared tokens that put it there), the
   lane this repo's routing picks, and the backlog's current shape. It returns
   `verdict: null` and that is not a placeholder: the engine computes what is WORTH READING
   and never decides. Nothing about a lexical overlap is a claim that two asks are the same.
2. **READ the candidates — do not skim the scores.** Open each one that could plausibly be
   the same work. A high score with unrelated intent is a miss; a low score on an epic whose
   description turns out to cover the ask is a hit. This is the judgment the surface exists
   to make cheap, and it is yours.
3. **Record the relationship you found**, rather than leaving it in the conversation:
   `add-epic … --link "relates-to:<id>:<why>"` where the two asks inform each other;
   `--link "supersedes:<id>:<why>"` where this ask REPLACES an existing epic — then end the
   superseded one with its own disposition (`--outcome superseded --reason "<what replaced
   it>"`), because a consolidation that leaves both epics open has consolidated nothing.
   A candidate `triage` marks `superseded: true` is already dead — do not consolidate into it.
4. **Decide the lane; do not inherit it.** `triage` already ran `suggest-lane` for you and
   its answer reads THE ASK — the words, the size, this repo's `laneRouting` overrides — and
   nothing else. It cannot ask what a person would ask, whether this work SERVES something
   already committed to, because pm holds no milestone or product context to weigh and the
   engine will not invent one. The suggestion is an input; the lane is your call.
   THE TIE-BREAK IS ASYMMETRIC, and it is not a matter of taste. `claude-code` means no spec,
   no plan, no gate and no stories — right for a genuine sub-2-hour tweak, and the reason a
   misrouted epic leaves no record of what it was FOR. Over-processing costs hours;
   under-processing costs the record permanently, and nothing later can reconstruct it. So an
   unresolved routing question resolves AWAY from `claude-code`, never into it.
   Whenever you register in a lane other than the one routing suggested, say why on the epic:
   `update-epic <id> --notes "lane: <chosen> not <routed> — <why>"`. The tracker-sync
   procedures below already demand that line; it binds every path that registers an epic,
   this one included. Measured in pm's OWN repository, not necessarily yours: 83% of epics sat
   in `claude-code`, 51 of them already archived, none carrying an artifact link.
5. **Say no out loud when the answer is no.** Not every ask should be taken on, and declining
   by never registering it destroys the record that anybody considered it. Register it, then
   `update-epic <id> --status archived --outcome declined --reason "<why not>" --no-deferrals`.
   Two commands, deliberately: creating an epic directly at `archived` stamps an engine record
   carrying no reason, which is the silence this step removes.

**This is not a substitute for the identity dedup in the sync procedures below, and they are
not a substitute for it.** A URL match answers "have I already mirrored THIS item"; triage
answers "is this ask already in the backlog under another name". Run both.

## Reporting — pm owns what is recorded and what is said; you own how you say it

This section governs how you REPORT. It never governs what the sections above instruct you to
DO: a brevity contract shortens prose, it does not authorise skipping a required task item, a
gate, or a recorded disposition.

1. **A recorded fact is not output, and no contract shortens it.** `--outcome` and its
   `--reason`, `--no-deferrals` or the deferrals it stands in for, a gate verdict,
   `--attribute-commit`, `--notify`, `record-reconcile`, `record-cross-spec-review` — these
   are WRITES to `.conductor/state.json`, not sentences. Applying a communication preference
   to one is data loss, not brevity.
2. **A report another AGENT reads back is a wire format and does not bend.** The
   `hierarchy-child-executor`'s `STATUS/DONE/DECISIONS/CONCERNS` block, the
   `merge-conflict-resolver`'s, and the `reconciler`'s `VERDICT/AMENDMENTS/NOTES`: the
   orchestrator branches on `STATUS`, and `VERDICT`'s value space is enforced by
   `record-reconcile` one hop later. Keep those field names and that order exactly. The PROSE
   INSIDE a field is ordinary writing and follows item 3 like anything else.
3. **Everything a HUMAN reads follows the user's contract, not pm's.** The consolidated
   end-of-hierarchy report, the end-of-epic autonomy report, the preflight question batch, a
   gate summary, `/pm:status` narration, `/pm:next`'s recommendation. If the user
   has an output style, or a communication contract in their CLAUDE.md, render pm's
   human-facing output in THAT shape. pm's headings are a DEFAULT for a user who has
   configured none — not a house style that outranks one. Two competing formats in one
   session is the defect.
4. **Map the content into their shape; never drop it to fit.** Reshaping is always allowed;
   omitting is never. Where the user's shape has no slot for something pm requires — the
   `notifications[]` read-back, the explicit "are you OK with these?" checkpoint, the
   deferral list, a blocked child, a `CONCERNS` line worth flagging — ADD a slot rather than
   drop the element. Silently deleting an obligation to fit a terse contract is the same
   failure as imposing pm's format over theirs, pointed the other way.
5. **CLAUDE.md is the only channel that reaches a subagent.** A subagent inherits every level
   of the CLAUDE.md hierarchy the main conversation loads, `~/.claude/CLAUDE.md` included; an
   OUTPUT STYLE applies to the main conversation ONLY and does not reach one. So when you
   dispatch a `hierarchy-child-executor` or the `reconciler` and the user's contract lives
   only in an output style, carry it into the dispatch prompt yourself — otherwise the child
   cannot honour a preference it was never given.

## Epic-level autonomy

An epic's `autonomy` block (`.conductor/state.json`) can grant it broad execution trust —
`level: "off"` by default (today's behavior, unchanged). Setting `level: "autonomous"`
removes the need to ask before each phase transition, but NEVER removes a genuine safety stop.
This is development-time only — it never covers actions with irreversible EXTERNAL side
effects (sending email/Slack, deploying to production, third-party API calls, pushing to a
shared branch); those are out of scope regardless of autonomy level.

1. **Preflight before flipping the switch** — see the `conductor` skill's
   "Epic-level autonomy — the preflight scan" section for the full process. In short: read
   the epic's full source, produce a short batch of destructive-risk-points +
   genuine-unknowns questions, get the user's answers, THEN record them:
   `set-autonomy <id> --preauthorize "<action>:<reason>"` / `--context "<note>"`, and only
   then `set-autonomy <id> --level autonomous`. For routine, repeated categories of action
   instead of enumerating each one, use the shorthand
   `--preauthorize "category:<filesystem|network|schema|external-api>:<reason>"` — see the
   `conductor` skill's "Epic-level autonomy" section for the exact keyword heuristic each
   category matches at decision-rule time.
2. **Execution-time decision rule** — check every destructive action against these, in
   order, before treating it as a stop:
   a. Already pre-authorized in the preflight — either an exact `action` match or the
      action falls under a granted `category` (per the category heuristic)? → proceed,
      record via `--notify`.
   b. No backup/restore path exists? → STOP regardless of autonomy level.
   c. Destructive but restorable (backed up first)? → WARN — `--notify` it immediately, proceed.
   d. No context to act on? → STOP — a real gap, not a false stall.
   e. Consequential and not yet notified? → `--notify` it immediately, then proceed.
3. **Notify incrementally, not at the end** — `--notify` writes durably to `state.json`'s
   `notifications[]` the moment a WARN-class (c) or consequential (e) decision is made. Do this
   AS EACH DECISION HAPPENS, not batched — a session can be compacted or interrupted mid-epic,
   and anything not yet `--notify`'d is lost when that happens.
4. **End-of-epic report** — on completion, read back the accumulated `notifications[]` and
   report what was asked, what was done, and the decisions made in the user's absence (drawn
   from that log, not from memory), with an explicit "are you OK with these?" checkpoint, THEN
   run tests. Leave room to iterate — including rewriting code — if the user is not satisfied.

## Review mode

Review intensity is a bounded dial, not a free-form call each time — set via
`set-review-mode --mode <off|standard|thorough>` (default: `standard` if never set).

| Mode | Reviewer budget | Trigger |
|------|-----------------|---------|
| `off` | none — self-review only | tiny, low-risk, single-file claude-code tweaks |
| `standard` | one fresh-context reviewer per gate | the default: OpenSpec Gate 1/Gate 2, a Superpowers task review |
| `thorough` | two independent fresh-context reviewers per gate; adjudicate any disagreement yourself | schema/migration changes, security-sensitive work, or anything explicitly flagged high-stakes |

Current mode: **standard**.

## Feedback — don't let friction stay silent

If you hit a bug, a missing CLI verb, an unexpected limitation, or repeated friction
working with this plugin — in this repo or any repo using it — don't just work around it
and move on. File it: `/pm:feedback [bug|feature] "<summary>"` against `cfdude/pm`, or ask
the user "want me to file this as feedback?" if you're not sure it's worth it. The failure
mode this guards against is silent: hand-editing `.conductor/state.json` to flip a story's
`done` flag (no CLI verb exists for it) recurred across several separate sessions before
anyone reported it, even though `/pm:feedback` existed the whole time. A filed issue is
cheap; an unreported recurring papercut is not — silent pain is where a product fails its
users.

## Re-read the source before an epic becomes the work

An epic becoming active is the moment specs or a plan get drawn for it. Before that, re-read
what it is FOR. Which source depends on provenance, never on any tracker's direction:
- The epic has an `externalId` → re-read the LINKED ITEM (body, comments, labels, state), then
  record what you found: `record-tracker-refresh <id> --verdict unchanged|material-change
  --external-updated-at <iso> [--summary "<what changed>"]`. The timestamp is the tracker's
  own, never a local clock reading, and recording it clears the obligation.
- The epic has NO `externalId` → re-read its local source: its plan document, or its OpenSpec
  proposal plus its tasks. This one is instruction only — nothing is recorded in state for it,
  and `record-tracker-refresh` refuses such an epic by name rather than accepting a verdict
  about a linked item that does not exist.
An outward-mirrored epic owes the same look as an inward-born one: a linked item accumulates
third-party context regardless of which way it was born. Origin decides only whose ask wins
when the item and a local spec disagree.
<!-- END pm-conductor rules -->
