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
- Register new core integration test files in `crates/core/tests/integration.rs`; see the [testing handbook](docs/TESTING.md) for everyday commands, qualification boundaries, and test authoring standards.

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
