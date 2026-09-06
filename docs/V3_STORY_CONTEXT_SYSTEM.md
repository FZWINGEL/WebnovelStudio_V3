# WebnovelStudio V3 — Story Context Engine

**Design extension · 5 September 2026**
**Status:** adopted design. C0–C2 and parts of C3 are implemented: exact evidence/retrieval, frozen packets, adopted guidance, recent complete exchanges, linked retry guidance, saved discussion sources, and optional approved writing briefs. C4-A adds explicit single-chapter navigation memory with retained evidence and source inspection; C4-B supplies eligible current views to working author-room discussions when full prose does not fit, using immutable generated-view references, separate coverage, and exact source inspection. Selected-passage Apply/Reject, history restore, draft export, bounded Codex assistance, and author-only chapter review are integrated into the native development app. The F2-B core freezes exact earlier reviewed versions plus a current working target, with immutable historical reader positions; continuation UI/live dispatch remains open. C3 relevance selection, higher-level C4 digests, C5–C6, typed accepted story records, narrative evaluation, and full provider/native/release qualification remain open. Exact current test and hosted evidence is maintained in [implementation status](IMPLEMENTATION_STATUS.md).
**Extends:** `V3_ARCHITECTURE_REFINED.md`, especially §§9–11. Replaces the minimal context compiler in §10 with the design below.
**Companion:** `V3_STORY_CONTEXT_FIRST_SLICE.md`.

## 0. Decision and scope

Give WebnovelStudio a **persistent, evidence-linked Story Context Engine**. Store the whole story locally, represent it at several levels of detail, and compile a fresh working context for every model request. When a request needs more evidence, permit bounded, read-only exploration of the same frozen story snapshot before producing the answer or edit proposal.

The design goal is **maximum useful, current, permitted story coverage**, not maximum prompt length. The model should receive broad orientation plus precise evidence about the current task, while retaining a route to older details that the initial packet did not contain. When the entire eligible manuscript fits the selected model and author-authorized input budget, including it is a supported strategy—not something retrieval automatically forbids.

This extension preserves the existing desktop, Rust, SQLite, editor, autosave, Apply, Undo, project isolation, provider, and reviewed-story authority contracts. It does not add another manuscript, an automatic canon writer, an autonomous author, a graph database, a vector server, or a multi-agent framework.

### What “always has context” can mean honestly

| Property | Contract |
|---|---|
| Available locally | Current saved manuscript, retained revisions, notes, discussions, decisions, and reviewed story records remain in the project. This is independent of provider sessions. |
| Available to this request | A frozen source manifest identifies exactly which current or explicitly historical sources the request may consult. Other projects and unauthorized story material are excluded. |
| Supplied to the model | Each provider invocation records the exact delivered packet. “Available for lookup” is not displayed as “read.” |
| Understood and followed | A model-quality question, evaluated separately. Supplying evidence cannot guarantee that a model notices every implication or writes perfectly consistent prose. |

A context window is finite. OpenAI's current documentation, for example, describes limits that account for input, output, and applicable reasoning tokens; the engine must use the selected adapter's actual accounting rather than assuming one universal budget. [S1] Experiments in *Lost in the Middle* and *NoLiMa* found limitations in how their tested models used long inputs; these are reasons to qualify a model's effective working budget, not measurements of every September 2026 model. [S2, S3]

### Evidence and assumptions

This extension uses the supplied refined architecture, its first-slice plan, and the relevant narrative-state/context sections of the supplied research and original proposal. The supplied repository research is rationale for this adopted design, not newly verified validation or an executed novel-generation benchmark. Current primary documentation and papers are listed in §17. Unsourced mechanisms, defaults, and thresholds below are design decisions or proposed tests.

## 1. The author experience

The author keeps writing and discussing the story normally. No compulsory character forms, world bible, outline, manual memory maintenance, or model connection is introduced.

For a request such as “Continue the confrontation, but remember what Mei promised her brother,” the assistant receives the current passage, relevant preceding prose, the applicable story state, the promise and its exact setup, relevant accepted intentions, and a compact view of the wider story. It can inspect additional permitted evidence before proposing text. The author does not have to remember the promise's chapter number.

The assistant panel adds **Story context**, with four understandable views:

| View | What it shows |
|---|---|
| Used | Exact passages, summaries, rules, decisions, and notes supplied in this request. |
| Available | Further sources that this request can look up; not a claim that the model has read them. |
| Not included | Meaningful budget omissions and policy exclusions, without exposing forbidden secret content to a restricted writing call. |
| Needs refresh | Relevant changed sources, incomplete analysis, conflicting records, or missing evidence. |

A note or passage can be pinned with **Use this for this request**, **Use for this chapter**, or **Use for this project**. Pins carry scope and audience; they do not override secrecy or source-validity rules. A mandatory pin that cannot fit blocks submission with an explanation. It is never silently dropped.

The default request strategy is **Story-aware**, chosen automatically by task. An optional **More story context** control increases the author-authorized input allowance and permits additional retrieval rounds when the provider supports them. It never silently changes the model, reasoning settings, source basis, or disclosure policy. The author continues to see the actual selected model and supported traits.

There is no “100% remembered” badge. A useful receipt might say: “Used the complete current chapter, 6 earlier passages, the current cast notes, and summaries covering chapters 1–18. Chapters 19–20 have no current summary; their original text remains searchable.”

## 2. Architecture and ownership

```text
Author's saved documents + decisions + valid reviewed story records
                              |
                 Rust project source reader
                              |
          +-------------------+-------------------+
          |                                       |
  Original evidence                       Derived memory views
  exact revisions/spans                   passage/alias indexes
  discussion messages                     summary hierarchy
  accepted rule versions                  temporal/entity views
          |                                       |
          +-------------------+-------------------+
                              |
                Frozen StorySnapshot + policy
                              |
                    Rust ContextCompiler
           mandatory inputs / coverage / retrieval
                              |
                ContextPacket + durable receipt
                              |
                    Existing provider adapter
                              |
                optional needs-context request
                              |
         Rust read-only tools on the SAME snapshot
                              |
            enlarged/repacked packet, within budget
                              |
             discussion or inspectable proposed edit
                              |
                existing author-controlled Apply
```

**Rust owns** source eligibility, snapshots, search, temporal queries, packing, receipts, read-tool authorization, budgets, and memory installation. **JavaScript owns** presentation, the live editor, selection, and the context inspector. **The model proposes** summaries, claims, retrieval questions, answers, or edits. It has no direct database, filesystem, canon, or manuscript-write capability through this subsystem.

Use the existing project connection owner and job supervisor. Indexing and memory jobs yield to saves and Apply. No database transaction waits for a model. A project switch never changes an existing job's project binding.

Expand the existing `context.rs` into a small module, without adding a new crate:

```text
crates/core/src/context/
  mod.rs          # public entry points and DTOs
  snapshot.rs     # frozen source manifests and eligibility
  index.rs        # passage projection, aliases, local search
  memory.rs       # derived summaries/observations and freshness
  retrieve.rs     # evidence bundles and bounded relation expansion
  compile.rs      # task recipes, coverage, token packing
  tools.rs        # read-only request-scoped tool execution
  receipt.rs      # exact delivered packets and omissions
```

Narrative authority and readiness remain in `story/`. Provider protocol details remain in `providers/`. The context module must not become a second domain engine.

## 3. Memory is not one rolling summary

Use four complementary representations. They point to the same evidence, rather than competing as sources of truth.

### 3.1 Original evidence: the durable foundation

Keep the canonical document JSON and immutable revisions under the existing document contract. Searchable prose is a projection with a map back to stable block IDs and exact revision spans. Preserve supported formatting and scene breaks; never reconstruct the manuscript from memory summaries.

Also retain the original text of relevant notes, author decisions, and conversations. A model response is an assistant statement, not established story truth. An unaccepted alternative is excluded from current-story context unless the author explicitly asks to compare alternatives.

**Old details do not disappear because they are absent from a summary.** They remain in exact source text and can be retrieved later. That is an availability guarantee, not a guarantee that a query will always retrieve the right passage.

### 3.2 Multi-resolution summaries: breadth

Represent the story at optional levels:

```text
Project orientation
    -> author-defined arc, volume, or part, when present
        -> chapter
            -> named scene or derived passage group
                -> exact paragraphs
```

Do not require authors to create arcs or scenes. Without that structure, derive passage groups from document blocks and use ordered chapter ranges for retrieval navigation. Generated groups are index units, not new manuscript scenes.

A chapter memory card contains a short orientation and source-backed sections for significant events, relationship changes, outstanding goals, unresolved promises, relevant state changes, and uncertainties. Omit empty sections. A scene card can be more detailed around emotional movement, motivation, staging, or a recurring image. None of these categories is compulsory in the author's writing interface.

Maintain two deliberately different summary classes:

| Summary class | Treatment |
|---|---|
| Accepted narrative summary | Reviewed content already supported by V3. Immutable, source-bound, not disposable; active only under the existing authority rules. |
| Generated navigation digest | Unreviewed retrieval aid. Source-bound, replaceable, clearly labeled. Useful for orientation and finding evidence; never sufficient on its own to establish canon or override prose. |

Each factual digest item links to source evidence. A higher-level digest keeps the union of its source dependencies, including child digests and underlying revisions. Rewriting a digest cannot clear those dependencies or lower its secrecy classification. Important factual answers and constraints should reopen the underlying evidence, not merely cite a summary of a summary.

Do not implement memory as `new_summary = summarize(old_summary + new_chapter)` forever. The rebuild path always starts from source-bound chapter/scene material. Multi-level retrieval is research-motivated: RAPTOR studies retrieval across levels of abstraction, but its QA results do not validate this specific writing design or eliminate summarization errors. [S4]

### 3.3 Entity, time, and obligation views: persistent details

Reuse V3's reviewed story records and add derived views over them. The useful distinctions are:

| View | Questions it should support |
|---|---|
| People and relationships | Who is this person? What are their aliases, goals, conflicts, and changes in attitude? |
| Objects and locations | Who last possessed the key? What evidence records a transfer? Where was the object last seen? |
| Rules and terminology | Which accepted rule applies here? Which name, title, honorific, or ability spelling should be used? |
| Beliefs and knowledge | Who believes the tower is empty? Who has actually learned otherwise? |
| Events and dependencies | What happened, in what known order, and what later passage relies on it? |
| Promises and open threads | What setup is unresolved? What was promised to a reader or another character? Was a payoff accepted? |
| Intent and style | What has the author explicitly asked to preserve or avoid? Which prose samples illustrate the chosen voice? |

Do not turn all prose into triples. A readable statement, typed record kind, entity references, source anchors, and optional temporal fields are sufficient for most entries. Add constrained predicates only for checks the product actually performs, such as possession or an explicitly recorded learning event.

An open promise does not expire after a fixed chapter count. A character's last known injury or an accepted magic-system constraint can remain important long after its last mention. Retrieval must not use recency as the only priority.

The 2026 *Narrative World Model* paper is particularly relevant to narrative decomposition, query-conditioned evidence retrieval, and separating events from knowledge and revelation. Its generation evaluation is future work, and its limitations qualify the interpretation of typing and benchmark results. This proposal borrows the problem decomposition, not a claim that importing that system would improve complete novels. [S5]

### 3.4 Task working context: depth

Every request receives a newly compiled packet containing the exact target, current instructions, protected scope, relevant story state, appropriate orientation, and selected original prose. This is the only layer directly serialized into a model invocation.

The same permanent store can therefore support broad plotting, a pacing discussion, a precise sentence replacement, and a question about an earlier chapter without forcing them to share the same prompt.

## 4. Authority, provenance, and time

### 4.1 Keep authority separate from usefulness

Use the following classifications in memory results:

| Classification | Meaning and permitted use |
|---|---|
| Explicit author rule/decision | The author adopted it for a specified scope. Active versions come from the existing rule/decision heads. |
| Reviewed story record | A fact, belief, disclosure, promise, or summary active through a valid selected ready bundle. |
| Current draft evidence | Exact saved prose from the selected working basis; usable as draft context, not advertised as reviewed canon. |
| Generated observation | A tentative interpretation with exact evidence links. Can guide retrieval; cannot become an authoritative constraint automatically. |
| Plan or alternative | An intended possibility, not an event that already happened. |
| Historical/superseded | Retained for comparison or explicit historical requests, not silently mixed into the current story. |

The distinction between *what the text says* and *what is true in the fictional world* is essential. A quoted lie, an unreliable narrator, a dream, or a character's mistaken belief is not automatically a world fact. Citation presence verifies a link, not an interpretation.

A generated observation may suggest `Mei possesses the key`, but accepting a new canonical state still follows V3's author decision and ready-bundle transaction. Conversely, draft assistance does not require authors to approve every extracted observation: it can work directly from saved prose, with uncertainty made visible.

Do not resolve an explicit rule/prose contradiction with a hidden precedence override. Show both sources and the conflict. The author can choose a correction or an intentional scoped exception.

### 4.2 Three coordinates, plus the chosen basis

Keep these independent:

- **Editorial basis:** exact document revisions, active rule/ready-bundle versions, and ordering epoch.
- **Story time:** an optional event, interval, or partial ordering within the fictional world.
- **Disclosure/knowledge boundary:** where the reader and each relevant character have learned something in the chosen manuscript order.

A theft can occur on story day 3, be disclosed in chapter 20, and be learned by Mei in chapter 25. A chapter 10 scene must not acquire that information merely because the event happened earlier in story time.

Do not reduce temporal state to “take the most recently inserted row.” A flashback or an earlier chapter revision breaks that rule. Derive state from eligible evidence and explicit temporal/supersession relations. If an interval or ordering is not known, return an ambiguity rather than inventing a timestamp.

State lookup should return **last established state at this boundary**, its source, subsequent eligible conflicting/transition evidence, and any coverage gap. It must not claim complete present state just because extraction found no later update. Only predicates with an explicit persistence rule can be carried forward as known state; otherwise use “last observed.”

### 4.3 Alias resolution

Store explicit aliases for romanized names, transliterations, titles, and English alternatives. Automatically discovered aliases are suggestions or search expansions until accepted. Identical names do not prove identical characters. Entity merges and splits preserve evidence and invalidate affected projections rather than rewriting past decisions in place.

## 5. How memory stays current

### 5.1 Every successful save: deterministic local maintenance

The save transaction remains the authority boundary. It updates the working document and marks its derived projections dirty. No model call is part of saving.

After commit, a low-priority local worker refreshes changed passage text, block-to-text maps, literal/FTS indexes, explicit links, and deterministic alias matches. It also identifies digests and contextual views whose dependency hashes no longer match. These operations do not require a provider or usage charges.

Changed documents remain available through exact source reads even while indexes rebuild. A context request searches current unindexed text through a slower local fallback, or explicitly reports an incomplete search. It may not silently treat a stale index's empty result as “the detail does not exist.”

### 5.2 Semantic analysis: optional and source-bound

Semantic extraction and abstractive summaries need a model unless the author writes them. Offer **Refresh story memory** for a chapter or selected range. A project preference can explicitly authorize analysis on meaningful checkpoints, with a selected model and usage cap. It is off by default, never runs on every autosave, and never automatically launches a paid critique after each chapter.

Without semantic analysis, the system still works through exact prose, accepted notes, local retrieval, and deterministic navigation cards. It should show that broader semantic orientation is limited rather than fabricate summaries.

An analysis job reads a frozen source and returns a proposed digest plus observations, evidence anchors, uncertainties, and candidate entity links. Rust verifies schema, referenced source membership, quote/range integrity, and size limits. These checks cannot prove semantic truth.

Installation is a separate short transaction:

1. Verify the recorded source and contextual dependency versions still match the intended basis.
2. Install the result as a generated digest/observation set, not accepted canon.
3. Record its recipe, producer model/traits, input manifest, and job identity.
4. Update derived-view availability only after commit.

If the source changed during analysis, retain the result as historical job evidence. It must not replace current memory. A retry is explicit or falls under the author's previously granted bounded analysis policy; it is not a silent paid retry.

### 5.3 Local versus contextual freshness

A text-only chapter digest depends on that chapter's exact revision. A claim such as “This resolves the promise from chapter 3” also depends on the earlier promise and story basis. Mark those different dependency sets explicitly.

**Current C4-A/B implementation limitation:** generated chapter views also require the project-wide source epoch to match. Consequently, an unrelated story edit prevents reuse until a new explicit refresh. This is a conservative intermediate rule, not the intended final chapter-only freshness behavior. The next memory increment should qualify exact dependency freshness for the closed single-chapter recipe while preserving the collection-level epoch for request/proposal staleness and contextual interpretations. Disclosure and namespace checks remain mandatory.

Changing chapter 8 can leave chapter 12's literal text index usable while invalidating its continuity interpretation. Recompute known dependents; preserve V3's conservative later-chapter review fence for unrecorded impacts. Missing dependency links do not prove independence.

No background extraction automatically clears a review fence or revalidates ready bundles.

## 6. Freeze a story snapshot before any request

The current chapter is not the only source that needs versioning. Freeze a **StorySnapshot** containing the selected working/reviewed basis, eligible document revision IDs, active rule/decision versions, ordering epoch, disclosure policy, and available derived-view versions.

The frontend first flushes the target document and any request-linked composer changes. The existing save acknowledgment contract remains unchanged. Then the project connection owner performs a transaction that reuses existing checkpoints and creates immutable checkpoints only for selected saved working versions not already represented. It records the source manifest and authority heads together. There is no model work or full index rebuild in this transaction.

The first freeze of a large imported project may copy substantial text. Precheckpoint imported sources during import and qualify this cold path; subsequent requests usually checkpoint only changed documents. If cold snapshot creation materially delays saves, pre-stage per-document checkpoints in bounded transactions and finalize only after an epoch/head consistency check. Do not silently assemble a cross-version mixture to avoid the wait.

A saved snapshot is immutable and requires no long-lived SQLite read transaction. Later lookups use its pinned revision IDs, not mutable document heads. New typing and new memories do not silently change a running request. On restart, the source manifest and all delivered packets remain explainable.

**Working basis:** current saved drafts may be included, labeled as such; later/future material still obeys the request's audience policy. **Reviewed basis:** only valid selected ready bundles and explicit adopted rules/decisions qualify, with the target handled under the existing reviewed-continuation contract. **Historical basis:** available only when explicitly selected, with a clear historical label.

At Apply, compare the original target version and existing relevant context/authority/order preconditions. The new engine does not introduce automatic rebasing. A fresh memory index does not make an old suggestion fresh.

**Collection-level changes also matter.** A lookup can depend on not finding a later transfer, not just on the passages it returned. Add a monotonic `context_source_epoch` to the project, incremented in the same transaction as changes to story-source bodies, relevant metadata, author guidance, active authority, or ordering. The first implementation binds prose proposals to this epoch as well as the target version: any source-corpus change makes them stale, even if the changed document was not retrieved. This intentionally over-invalidates rather than missing a newly relevant source. Pure index rebuilds and unaccepted generated-view installation do not change this epoch. Later selective invalidation must track query/source-set dependencies, including negative lookups, before replacing this conservative rule. Historical discussions may remain readable, but their results are explicitly historical.

## 7. Compile breadth and depth, not arbitrary top-k chunks

### 7.1 Task recipes

| Task | Always include when applicable | Broader context policy |
|---|---|---|
| Brainstorming/planning | Current request, selected idea, adopted creative constraints | Author-room overview, related notes, alternatives explicitly requested, future intentions. |
| Whole-chapter discussion | Entire chosen chapter, author feedback, relevant conversation decisions | Wider arc, promises, relationship history, earlier/later prose permitted for author-room analysis. |
| Passage revision | Exact selection, editable scope, protected surrounding structure, approved instruction | Read-only neighboring prose, applicable terminology/rules, relevant state and evidence. |
| Continuation | Exact current ending or scene prefix, task intent, applicable state, POV/disclosure boundary | Broad prior-story orientation, relevant older evidence, active goals/promises, selected style samples. |
| Story question | Question, named entities/time/basis, exact evidence relevant to the answer | Targeted timeline/relation expansion; do not fill with unrelated lore. |
| Whole-book analysis | Explicit book/range and analysis objective | A labeled coverage plan; multi-pass analysis only with author authorization if the whole source cannot fit. |

Current instructions and preserved decisions beat redundant old chat. They do not override explicit manuscript-edit scope or secretly change canon. Read authority and write authority remain separate even when an entire book is available for discussion.

### 7.2 Two packing paths

**Full eligible-source path:** when the selected current material fits the model's qualified input allowance, include its full text with a compact navigation/state section. Do not also inject every summary redundantly. Archived alternatives, future secrets, and superseded versions are not eligible simply because there is space.

**Multi-resolution path:** when it does not fit, provide broad coverage at lower detail and targeted evidence at higher detail. Begin with the mandatory target and constraints. Represent available earlier narrative ranges with valid summaries, then expand the most useful ranges to chapter, scene, or verbatim detail. An expanded range replaces its coarser redundant representation unless both serve different explicit purposes.

Track coverage as disjoint source ranges and granularity, not a fixed “last three chapters” rule. An old thread can receive more detail than a recent unrelated chapter. If even a truthful broad overview cannot fit, report incomplete coverage and retain navigable lookup handles. A title alone is a directory entry, not semantic coverage of that chapter.

Never invent a digest to fill an unanalysed range. Fall back to exact text, an honest navigation entry, or a disclosed gap. Higher context allowance upgrades original-text coverage rather than padding the prompt with duplicate summaries.

### 7.3 Token budget

Let the adapter calculate a prompt ceiling from its documented model limits and selected traits. A generic accounting sketch is:

```text
prompt_ceiling = min(explicit_input_limit_if_any,
                     total_context_limit - reserved_generation - safety_margin)

story_budget = prompt_ceiling
               - instructions_and_task
               - permitted_chat_history
               - tool_schemas_and_protocol_overhead
```

`reserved_generation` includes reasoning according to the adapter's actual accounting; do not subtract it twice when it is already included in the output allowance. Tool results and repeated turns consume the same context constraints, not free extra capacity. Count the exact serialized request when possible; label conservative estimates when an exact tokenizer or CLI wrapper accounting is unavailable. Do not treat an English words-to-tokens ratio as exact token accounting.

For illustration only, a 128,000-token total allowance, 12,000-token generation reserve, 8,000-token safety margin, and 8,000 tokens of non-story input leave 100,000 tokens for story material. These are not model recommendations or universal defaults. If the eligible story has only 26,000 tokens, send useful content once rather than filling the rest.

If mandatory content cannot fit, do not send a truncated target or quietly change “whole chapter” into “selected parts.” Return an explicit budget conflict. The author may choose a larger qualified model, a narrower scope, or a clearly named multi-pass analysis.

## 8. Retrieval: find evidence, then follow its consequences

### 8.1 Retrieval channels

Use ordinary project-local queries, in this order:

1. **Exact references and adopted pins:** target, linked passages, mentioned chapter IDs, accepted rules, and specific source requests.
2. **Entity and state lookup:** explicit aliases, relevant belief/state records, open promises, and accepted creative decisions.
3. **Local text search:** lexical search for prose plus literal/substring search for short names, aliases, quotes, and terminology.
4. **Evidence expansion:** the surrounding permitted passage, a linked setup/payoff, a possession transition, a knowledge-acquisition event, or a directly recorded dependency.
5. **Optional semantic retrieval:** introduced only after measured paraphrase-retrieval failures, using the same source, time, and audience checks.

SQLite FTS5 provides a suitable local starting point. Its trigram tokenizer's full-text queries do not match substrings shorter than three Unicode characters, so short names need explicit alias/literal lookup. A parameterized `instr(text, query) > 0` scan over the eligible source set is a simple fallback; do not confuse an FTS limitation with absence from the manuscript. [S6]

A normalized search key is not the manuscript. Preserve original code points and convert results back through the versioned text-to-block map. Evidence anchors reuse V3's UTF-16-within-block convention and exact quote/hash validation. Do not interpret a search-library byte offset as an editor position. Repeated quotations return separate source spans; selection identity comes from the chosen revision and anchor, not first-match replacement.

A fixed-size text chunk is a storage/search unit, not necessarily the right answer unit. Search can seed a paragraph; the evidence bundle should also carry relevant neighboring context and linked state transitions, subject to the same disclosure check. Chunk overlap is deduplicated before packing.

### 8.2 Do not retrieve half of a state change

For “Who has the key?” retrieving the initial gift is insufficient when a later eligible passage records a transfer. State retrieval asks for the applicable claim and its known supersession/transition evidence as a bundle. If sources conflict or chronological order is unclear, the bundle carries the alternatives and uncertainty together.

Likewise, a promise bundle contains its setup and any eligible payoff, cancellation, or change of intention. A belief bundle distinguishes what the character believes from evidence about world truth. A stale record cannot win because its embedding score is high.

The first implementation follows direct relations and a bounded second expansion when a named evidence need requires it. It does not recursively traverse the entire story graph. Scope and token budgets bound expansion; a truncated chain is disclosed. This is relational query logic over SQLite tables, not a requirement for a graph database.

### 8.3 Deterministic packing priorities

Apply hard eligibility filters before selecting or presenting candidates, and revalidate every expanded result. Search may use a shared local index, but the model sees only eligible source content and eligible directory metadata. Do not filter only after a global top-k if that would starve the permitted corpus of candidates.

Mandatory material is not part of a popularity contest: exact target, author instruction, editable scope, relevant adopted hard constraints, required knowledge boundary, and explicit mandatory pins either fit or submission stops.

For discretionary bundles, use a deterministic ordering: explicit task links; entities/threads involved in the current scene; relevant transition/causal evidence; appropriate prior prose; broader narrative orientation; style exemplars; secondary notes. Within a priority, prefer greater new coverage per token and use stable source ordering for ties. Recency is a secondary signal, not a deletion policy. Preserve both sides of a conflict as one bundle.

Start with inspectable rules rather than a learned reranker. Log which rules selected an item. Tune allocation and ranking on real author tasks, not fabricated significance weights.

### 8.4 Embeddings are a replaceable recall improvement

Add an embedding index when a labeled test set shows that names, aliases, links, lexical search, and expansion miss important paraphrased evidence. Compare retrieval improvement at matched packet sizes using English author tasks; no cross-language evaluation is required. Vector similarity does not imply entity identity, factual validity, currentness, or permission.

An optional index stores the source hash, embedding model/version, dimension, and normalization settings. Rebuild or partition it when these change; never mix incompatible vectors. A local index can initially use a simple bounded scan if measured latency permits. Do not select a separate vector database before profiling requires one.

## 9. Read more when the initial packet is insufficient

### 9.1 Request-scoped read tools

Provide a small read-only interface through the existing provider abstraction:

```typescript
// Illustrative contracts. IDs are opaque handles issued by Rust, not paths.
type StoryRead =
  | { kind: "search"; query: string; entityIds?: string[] }
  | { kind: "read"; sourceHandle: string; detail: "summary" | "passage" | "chapter" }
  | { kind: "state"; entityIds: string[]; question: string }
  | { kind: "threads"; entityIds?: string[]; status: "open" | "resolved" | "both" };

type ReadResult = {
  snapshotId: string;
  evidence: Array<{
    handle: string;
    text: string;
    sourceLabel: string;
    authority: "reviewed" | "draft" | "observation" | "intention";
    representation: "verbatim" | "digest" | "state_view";
  }>;
  truncated: boolean;
  gaps: string[];
};
```

The run binding supplies the project, snapshot, maximum time/disclosure boundary, and budget. The model cannot pass a different project path, expand its own audience, select arbitrary SQL, or request a write. A state question can narrow the permitted time range; it cannot widen the snapshot boundary.

Search results include short evidence snippets and handles. A handle exists only within the authorized snapshot. Each `read` rechecks source membership, visibility, and remaining budget. A changed disclosure-policy epoch stops further reads/submissions until a new request is prepared; a run cannot retain newly revoked access merely because its story text was frozen. Content already sent to an external provider cannot be recalled by this local policy change. Failures return structured missing/stale/disallowed/budget results, not invented memory. Directory titles and relation endpoints obey the same restrictions as source text.

The returned state text is a deterministic rendering of eligible records; complex semantic inference remains visibly model reasoning or uncertainty, not a hidden second “state truth” model.

### 9.2 Provider differences remain real

For a provider with qualified function/tool calling, the model can ask for these reads and continue after Rust returns evidence. OpenAI's documented tool flow explicitly separates the model's requested call, application-side execution, and a subsequent model request carrying the result; this does not require an agent framework. [S7]

For a provider without tool calling but with qualified structured output, use a mutually exclusive result envelope: `needs_context`, `discussion`, or `proposals`. A `needs_context` response contains only allowed read requests; Rust validates them, performs the reads, and makes a new model invocation. Ordinary prose containing a tool-like string is never executable.

For a provider with neither qualified mechanism, use the deterministic compiled packet and one normal model call. Display that automatic follow-up lookup is unavailable. The author can explicitly request more context and make another call; do not pretend that a generic CLI can perform supported function calls.

A CLI receives only the authorized packet, using its qualified input framing. Do not point a general coding agent at the live project folder to obtain “maximum context.” Filesystem/shell tools, inherited project instructions, hidden session memory, or automatic compaction must be disabled or controlled by the qualified adapter. An adapter that cannot enforce these boundaries is not eligible for restricted prose-generation mode.

### 9.3 A bounded loop, not an endless research agent

The initial default authorization is **one generation invocation, with up to two additional context-expansion invocations if needed and supported**. The author sees this potential before submission through the request's usage setting. A one-call setting is also supported. These are cost-control defaults, not research-derived optimal counts.

Batch related read requests in each expansion. Bound both invocation count and total estimated input/output usage across the job. Repeated input counts toward the job allowance; caching does not make it zero. Unknown prices are reported as unknown, with token/invocation limits still enforceable.

There is no mandatory paid “context planner” before every response. The initial compiler is deterministic. The model asks for more only when it chooses to, and may still fail to notice missing evidence. Tests must therefore measure missing-need detection separately from retrieval quality.

Before a follow-up call, Rust persists the exact new packet and invocation record, then submits it. Persist permitted provider response items needed by that adapter's protocol; do not discard required tool/reasoning state and pretend a reconstruction is identical. When a tool transcript would overflow the window, do not silently trim it. End the loop with a visible limit, or start an explicitly authorized fresh invocation from a newly packed evidence packet. A fresh invocation is not a resumed provider session.

No newly retrieved text appears magically in an already-running token stream. Additional evidence is incorporated at supported tool/turn boundaries. Streamed discussion and candidates remain provisional; manuscript application remains an author action.

### 9.4 Questions to encourage, not guarantees to claim

The request instructions encourage the model to check known state and exact source evidence before relying on an old item, revealing a secret, resolving a promise, or asserting an important remembered event. It should distinguish “not found,” “not indexed,” “ambiguous,” and “contradicted.” It should ask for a lookup or admit uncertainty instead of inventing a confident answer.

Require source handles for story-factual explanations and important continuity assertions where practical. Rust can validate that a cited handle was delivered, but not that the citation truly entails the claim. New fictional invention in a proposal is allowed; it must not be mislabeled as an established remembered fact.

## 10. Rich context without teaching the POV forbidden secrets

The author-facing assistant and the prose-producing request do not automatically share a transcript or provider session.

**Author room:** can discuss selected future plans, private notes, rejected alternatives when requested, and world secrets. It can know much more than the scene's characters. Its output is discussion or a revision plan, not immediately authorized prose.

**Writing view:** uses a selected narration policy, the scene's POV where applicable, reader disclosure frontier, approved source grants, and safe instructions. Raw author-room messages, future plans, or privileged digests are not automatically copied into it.

The default for a limited-POV scene excludes author-only secrets from the writing packet. A reader-visible fact still does not become POV knowledge. Evidence from other POVs or an omniscient narrator may be useful to an author-room discussion; including it in a writing request requires an explicit narration/source policy. A known POV fact can be included as knowledge, a belief as belief, and permitted environmental evidence as observation. Do not infer that every character present in a chapter knows everything described there.

An author may choose an external or omniscient narrator, or explicitly grant a disclosure. Those are legitimate narrative decisions, not an unlogged “bypass.” The selected policy and granted sources are recorded in the receipt. Full-story inclusion only applies within that policy.

For example, the author can discuss “The mentor killed Mei's father; hint at it but do not reveal it.” A safe brief for a restricted scene might be: “The mentor briefly recognizes the pendant, then changes the subject. Mei interprets the pause as grief.” The author approves that brief; the restricted prose call receives it without the secret explanation. The approval grants the exact brief, not the whole privileged conversation. Creating that brief need not require another model call: the author can write it.

Generated material inherits the restrictive information scope of its inputs. A digest compiled using future chapters cannot become chapter-safe merely by attaching a chapter-8 label. Rebuild it from eligible earlier evidence, or require an explicit author-reviewed safe version. Provenance checks must include non-cited input sources, since they may have influenced wording.

If the mandatory target itself conflicts with the selected information boundary, surface the conflict. Do not silently redact target prose, weaken the boundary, or classify it as safe based solely on a model judgment.

This is information-flow risk reduction, not semantic proof. Incomplete author annotations, ambiguous prose, valid character inference, and model inference remain limitations. A character may legitimately infer a secret from clues; the interface should not equate inference with having been told. Source exclusion, explicit source grants, structured knowledge labels, inspection, and author review are complementary controls.

## 11. Conversation memory and long drafting sessions

### 11.1 Preserve decisions, not an endlessly growing transcript

A persistent conversation retains all messages locally. The current C3 slice compiles a bounded selection of recent complete exchanges plus separately adopted guidance. It considers up to four completed and delivered turns within 16 KiB of exact serialized records, from the same document thread, operation namespace, and current policy. It freezes original text, selected scopes, and message/source IDs, and reports omissions. The first selector uses recency; richer relevance selection remains later work. See [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md).

A message such as “Keep the ending, remove the sarcastic tone, and don't kill the sister” matters. The current discussion UI offers **Keep as guidance**, opens an editable form, and requires the author to save the exact text with a scope of Next request, This document, or This project. Direct entry, edit, and retire use the same immutable versioned record and Rust-owned operation receipt. A frozen guidance record is separately typed, included exactly as mandatory AuthorRoom packet input, and shown by the inspector; restricted writing excludes all current guidance. Request guidance is consumed atomically only with a successfully persisted discussion start, and failed starts retain it. An explicit linked retry of a stopped, failed, or interrupted discussion retains its exact original request-scoped guidance when those versions are still active. Feedback, selected scope, and ordered source pins must match the original request; editing any of them starts a new request. Current document/project guidance and current permitted story sources are compiled afresh. Newly waiting request guidance is reserved for the next new request. Edited or retired inherited instructions, revoked policies, completed runs, and recovered-copy links are refused. The composer stores the retry link with its draft and immutable save receipt, so navigation/reload preserves the choice. The original guidance-use receipt remains the only consumption record; retries do not consume it again. The confirmed instruction is an author decision, not merely a sentence inside a lossy chat summary, and it is not automatically a world fact.

The selected-passage `ProposeEdits` composer also supports an optional author-approved writing brief. **Adapt as writing brief** may start from either an author or assistant message, but the author must edit and explicitly approve the resulting text before sending. A direct brief is allowed as well. The optional `safeBrief` draft value is persisted by schema 11 with its approval state, including unconfirmed text; reload/restart does not silently approve unfinished text. Editing its text or the selected scope clears approval; switching to **Discuss** removes it, and ordinary edit requests need no brief. The restricted packet receives the exact approved wording as `approvedWritingBrief`; the origin message ID, original chat, private sources, source pins, and current guidance do not cross this boundary. The receipt retains the text, hash, and optional origin ID for local inspection. Retries preserve the exact identity, older receipts without the optional field remain compatible, while historical replay and recovered copies cannot authorize a new request. Creating or changing a brief does not advance the story source epoch, invoke a provider, run paid autosave analysis, or alter Apply. See [ADR 0008](ADR_0008_WRITING_BRIEF.md).

Conversation digests are a future generated navigation aid with message-range provenance. If a digest says the author selected option B, the original decision/message must support it. Unaccepted assistant inventions do not become story knowledge through chat compaction. Superseded preferences remain historical; their current scope/version decides whether they are included. Richer conversation selection remains future C3 work.

Persistent document/project source choices are implemented for AuthorRoom discussions. **Keep source…** opens an explicit confirmation form; **Include next time** remains a one-request choice. Rust merges current saved choices with transient pins into the frozen packet, retains exact mandatory-source receipts, and refuses unavailable or oversized required sources. The current target is already mandatory and is included once. Restricted edit requests exclude these saved discussion choices. Changed choices advance the source epoch, while retries preserve their original transient request identity. See [ADR 0007](ADR_0007_DISCUSSION_SOURCE_PINS.md).

### 11.2 Rebuild between tasks; do not depend on provider memory

A new conversation, restarted application, provider switch, or different selected model rebuilds context from the same local project evidence. Hosted conversation IDs and provider compaction are not the only copy of anything important. Do not reuse an author-room provider session for a restricted writing view.

Provider caching is an optimization only. For example, OpenAI documents reuse of matching prompt prefixes and model-dependent cache behavior. Stable eligible instruction/reference prefixes may help, but source updates, disclosure changes, and correct packet construction take priority over cache hits. The system continues to work on a cache miss and records actual usage when available. [S8]

### 11.3 Long proposed drafts

For an explicitly requested multi-segment chapter draft, keep generated material in the existing candidate workspace, not the manuscript or reviewed story store. Later segments can read the exact candidate prefix and a tentative candidate-state note. These are session-local proposal sources, visibly distinct from accepted story evidence.

Recompile between authorized segments using the same frozen story snapshot plus immutable candidate versions. Candidate observations never update global memory until the author applies and subsequently reviews the relevant text under the normal V3 rules. No automatic “accept segment, update canon, continue” loop is introduced.

Segment boundaries are an output-limit or author-choice mechanism, not a universal scene/chapter formula. A passage revision remains a single scoped proposal, even when large amounts of read-only context are supplied.

## 12. Minimal data and request contracts

### 12.1 Reuse existing authority tables

Keep `documents`, `revisions`, `story_records`, `rule_heads`, `ready_bundles`, `decisions`, `dependencies`, `messages`, `jobs`, and `context_receipts` as specified in the refined architecture. Do not add a competing mutable `current_story_truth` table.

The useful additions/extensions are:

| Entity | Important fields and constraints |
|---|---|
| `source_passages` | Source document/version/hash, projection version, stable block/range map, exact searchable text. Disposable; current rows validated against heads, snapshot rows against pinned revisions. |
| `entity_aliases` | Entity, alias/search key, source and acceptance status. Ambiguous matches are allowed; alias text is not globally unique. |
| `memory_views` | Kind, scope, basis hash, recipe version, payload, producer job, representation class, information-scope provenance. Generated views only; accepted summaries stay in existing authoritative records/documents. |
| `memory_view_sources` | View ID plus all input source versions/hashes and optional supporting spans. Unique source membership; includes influential sources beyond displayed citations. |
| `story_snapshots` / `snapshot_sources` | Immutable source/authority/order/policy manifest, context-source/disclosure-policy epochs, and pinned revision references. Same-project ownership checks are mandatory. |
| `context_sessions` | Existing root job, snapshot, task, narration policy, authorization envelope, current invocation ordinal. No separate workflow scheduler. |
| `context_packets` | Session and invocation ordinal, exact messages/options, source manifest, omissions, budget estimate/method, hashes. Immutable once submitted. |
| `context_reads` | Session, local read operation ID, arguments hash, returned source handles and exact result, truncation. Duplicate operations return the same result. |

Use uniqueness for `(session_id, invocation_ordinal)`, `(session_id, local_read_operation_id)`, and the generated-view recipe/source-basis key where reuse is intended. A repeated read ID with a different payload is an error. A new analysis run using the same basis can be retained as a distinct candidate; do not silently replace an author-reviewed summary.

Snapshots and receipts pin the revisions they reference against automatic pruning. Derived indexes can be deleted and rebuilt. Deleting generated navigation digests does not delete accepted summaries, prose history, or author decisions. Restoring a backup into a recovered project preserves the evidence but invalidates/rebuilds environment-specific indexes and checks source hashes before use.

### 12.2 Representative command contract

```typescript
type ContextRequest = {
  operationId: string;
  projectSession: string;
  targetRevisionId: string;
  selectionAnchorId?: string;
  task: "discuss" | "revise" | "continue" | "plan" | "story_question";
  basis: "working" | "reviewed" | "explicit_history";
  narrationPolicyId: string;
  instructionMessageId: string;
  safeBrief?: {
    text: string;
    originMessageId?: string;
    confirmed: boolean;
  };
  sourcePins: string[];
  modelSelectionId: string;
  authorization: {
    maxInvocations: number;
    maxTotalInputTokens: number;
    maxTotalOutputTokens: number;
  };
};

type ContextPrepared = {
  sessionId: string;
  snapshotId: string;
  packetId: string;
  usedSourceHandles: string[];
  availableDirectoryHandle: string;
  coverage: Array<{ rangeLabel: string; detail: "verbatim" | "digest" | "directory_only" }>;
  gaps: string[];
  tokenEstimate: { input: number; method: string };
};
```

Opaque identifiers refer to Rust-owned entities. Rust resolves and validates model traits, author authorization, scope, and source ownership; it does not accept arbitrary client claims that a source is reviewed or a secret is safe. Wire counters/versions follow the existing safe-integer or decimal-string convention; do not silently round database integers through JavaScript numbers.

`prepare_context` is local and idempotent by operation ID and logical payload hash. It freezes sources and persists the first packet but does not contact a provider. The existing explicit run-start command submits the prepared packet. Submission records the selected capability descriptor and checks that required source/authority heads still match; changed sources require a fresh preparation or an explicitly historical read-only discussion, not an unnoticed stale write proposal.

A `needs_context` turn records a child invocation under the same job and snapshot. Every packet is saved before its external submission. Stop prevents new invocations after the cancellation decision commits; it does not promise to undo upstream work or billing. Durable completion and cancellation retain V3's existing serialized race policy.

Useful errors are `TargetChanged`, `BasisInvalid`, `SourceUnavailable`, `BoundaryConflict`, `MandatoryContextTooLarge`, `SearchCoverageIncomplete`, `BudgetExhausted`, `UnsupportedContextTools`, and `OperationPayloadMismatch`. Do not reduce them all to an empty context or generic provider failure.

## 13. Concrete walkthroughs

### 13.1 An old promise matters again

**Illustrative story, not user manuscript data.** The author is working on chapter 120, `c120@r18`, and asks: “Make Mei's confrontation with her brother more emotional, but keep the ending.”

The request freezes snapshot `ss-52`. The compiler reads the exact chapter and identifies the two characters from explicit scene links, accepted aliases, and literal mentions. Its relationship/thread query finds a still-open promise established in chapter 14. The evidence bundle retrieves that exact promise, the separation in chapter 67, and relevant current relationship records. It also includes applicable voice guidance and the protected ending as read-only context. Current valid summaries provide broader orientation where they fit.

The assistant sees why the confrontation matters rather than only the latest few chapters. If it asks whether the brother ever learned why Mei left, `state` retrieves the eligible learning/belief evidence, or returns “not established in indexed records” with supporting coverage information. Absence is not turned into certainty.

The proposal is still bound to `c120@r18` and the explicit replacement scope. A larger emotional payoff is not permission to change the ending. Only an author-approved, validated suggestion can affect the manuscript.

### 13.2 More context must not introduce future knowledge

The same project contains a private note and chapter 166 revealing the mentor's crime. In an author-room conversation the author may discuss it. The later restricted prose request gets a separate snapshot/policy view and an approved safe brief. Chapter 166, its digest, related private chat, and a project-level digest influenced by that future revelation are not passed through automatically.

A retrieval request for the mentor cannot widen that boundary. Even an apparently helpful alias or directory title must not expose the secret. The author-room discussion remains available to the author but is not replayed into the restricted provider conversation.

If an authorized omniscient passage is desired, the author changes the narration/disclosure grant explicitly and creates a new request. This is a different permission decision, not an invisible consequence of choosing “More story context.”

### 13.3 An earlier revision changes remembered state

Snapshot `ss-60` originally used `c08@r12`, whose reviewed bundle recorded Mei keeping the key. Chapter 12's text and reviewed interpretation depended on that.

The author changes chapter 8 to `c08@r13`, giving the key to Jun, and reviews that new version. The normal story transaction selects the new bundle, records known impacts, and activates the later-review fence. The memory module marks the old possession view and dependent contextual digests ineligible for the new current basis. It does not delete chapter 12 or overwrite its prose.

A new working-basis query returns the revised transfer plus the conflicting later passage, labeled as unresolved draft continuity. A reviewed-story continuation cannot proceed on the invalid suffix as if nothing changed. The author can review the suffix, or explicitly choose working-draft assistance. An older request using `ss-60` remains inspectable as historical; its eventual suggestion fails current source/authority checks.

If the old FTS/vector index still returns `c08@r12`, revalidation excludes it as current evidence. The compiler reads `c08@r13` through the exact-source fallback while indexing catches up.

### 13.4 Concurrent typing and a late analysis result

An analysis job reads chapter 7 at working version 41. The author types, and version 42 saves successfully. The job later returns a chapter digest.

Rust verifies the analysis basis, finds the mismatch, and retains the digest as historical output for version 41. It does not mark version 42's memory current. A feedback request for version 42 uses exact version-42 prose, a valid regenerated digest if explicitly produced, or an honest digest gap.

The editor never receives a content replacement from a memory update. Autosave acknowledgments still only advance the existing saved-generation watermark. A renderer reload rebuilds the inspector from durable packet/job data, not an in-memory “memory finished” event.

### 13.5 A stop request races with a context lookup

Session `cs-9` has finished invocation 0 and returned `needs_context`. Rust performs local read `read-1`, then the author presses Stop.

The project job transaction linearizes cancellation before invocation 1 starts. The read receipt can remain, but no next paid invocation is submitted. If invocation 1 was already durably marked dispatching and actually submitted first, cancellation stops local acceptance/continuation according to the adapter and existing race policy; upstream termination or billing may remain uncertain.

A crash after a read receipt commits does not lose manuscript work. A crash after external dispatch but before a response receipt is an interrupted/unknown provider outcome, not permission for an automatic paid retry. Reopening the project can reconstruct which packet was sent and offer an explicit retry using the same frozen sources, or a new request using current sources.

## 14. Invariants, tests, and quality evaluation

### 14.1 Correctness invariants

| ID | Invariant | Highest-value test |
|---|---|---|
| C1 | No source from another project enters a packet or tool result. | Colliding document/alias names in two projects; switch projects during a lookup. |
| C2 | Every supplied source is exact, source-bound, and eligible for the snapshot/policy. | Stale FTS/vector hit; future chapter; forged handle; wrong revision hash. |
| C3 | Mandatory target, instruction, scope, and constraints are never silently truncated. | Tiny budgets, unusually long English text with Unicode names/emoji, oversized pinned note, wrapper overhead. |
| C4 | Memory generation cannot mutate manuscript, canon, or readiness. | Malicious/malformed analysis output requesting a write; extraction after a draft save. |
| C5 | Generated digests cannot become less restricted than their inputs automatically. | A future secret paraphrased into a project digest without an explicit citation. |
| C6 | Repeated commands/read IDs are idempotent locally, not promises about provider billing. | Duplicate read delivery, lost start acknowledgment, dispatch/crash boundary. |
| C7 | Current source and authority changes cannot be hidden by an old cache. | Revise early chapter, reorder chapters, change rule, restore backup, rebuild indexes. |
| C8 | Belief, observation, plan, and world truth remain distinguishable. | Lying dialogue, dream, false rumor, unreliable narration, planned death not yet written. |
| C9 | There is no fabricated certainty from an empty search or incomplete extraction. | Unindexed current source; alias absent; unknown timeline; conflicting possession records. |
| C10 | Read breadth never enlarges edit scope. | Whole-book context for a repeated English/emoji sentence; only selected occurrence may change. |
| C11 | The exact input of each provider invocation remains auditable. | Renderer crash between durable completion and visible notification; restart without provider session. |
| C12 | Stop and usage limits prevent unauthorized follow-up invocations. | Stop after tool result; duplicate/out-of-order tool event; repeated `needs_context` at limit. |
| C13 | New evidence can invalidate an answer even when no returned passage changed. | Add a later key transfer in a previously unretrieved chapter; the collection epoch makes the old prose proposal stale. |

Qualify deterministic invariants with no-network mocks and fault injection. Zero failures is an acceptance requirement for these tests, not a statistical claim that all real-world bugs or narrative leakage are impossible.

### 14.2 Retrieval and memory evaluation

Create a small author-checked English fixture set before selecting embeddings or complex ranking. Include exact and paraphrased retrieval, multi-hop possession changes, old unresolved promises, flashbacks, repeated names, romanized-name and alias ambiguity, false beliefs, early revisions, chat decisions, unavailable evidence, and secret contamination through summaries. Unicode names, accents, and emoji are robustness cases; mixed-language narrative evaluation is outside this product slice.

For each question, record expected evidence spans, forbidden evidence, the chosen story boundary, and whether the correct response is an answer, uncertainty, or a conflict. Measure:

- Gold evidence recall at the actual packet token budget, with all necessary hops counted.
- Irrelevant/duplicated packet content, stale-source inclusion, and forbidden-source exposure.
- Temporal answer correctness, valid abstention, and false claims of “never happened.”
- Token usage, number of provider invocations, retrieval latency, and author correction burden.

Source exposure and narrative leakage are different metrics: a packet can pass its metadata policy while a model still makes an inappropriate inference. Conversely, a poor answer may result from reading failure even when all gold evidence was delivered.

LongMemEval distinguishes extraction, multi-session reasoning, temporal reasoning, knowledge updates, and abstention, and separates indexing, retrieval, and reading. Those are useful evaluation dimensions here, but a chat-memory score is not a novel-writing score. [S9]

### 14.3 Test the generation claim separately

Compare the same writer model and tasks under: recent prose plus one rolling summary; full eligible text when it fits; deterministic multi-resolution context; and multi-resolution context with the optional read loop. Compare both matched token budgets and each strategy's natural usage. Record model version/traits, seeds where supported, and actual cost rather than declaring an unequal-budget comparison superior by default.

For passage edits and continuations, use blinded human comparisons for motivation continuity, voice, staging, emotional development, repetition, promise/payoff handling, and overall preference. Count factual contradictions and unexpected disclosures independently of prose quality. Keep author edit scope and story basis identical across conditions.

Run ablations that remove source evidence, temporal relations, summary orientation, and read expansion separately. A good QA score with worse prose is not an acceptable writing-system improvement. Report variance across runs, not one attractive generated chapter.

### 14.4 Architecture-changing experiments

| Experiment | Default | Evidence that changes the decision |
|---|---|---|
| Full eligible context versus packed context | Support both; choose by fit, task, and qualification | Reproducible model/task gains favor one route at comparable quality/cost. |
| Lexical/link retrieval versus embeddings | Local exact/alias/FTS first | Material gold-evidence recall failures on English paraphrase cases, fixed by a qualified embedding route. |
| Generated summaries versus exact-only context | Optional digests, exact-source fallback | Human trials show sufficient benefit to justify opt-in checkpoint analysis and its maintenance cost. |
| Read-loop usefulness | Bounded optional expansion | Missing-need detection or latency is too poor; retain deterministic packet mode instead. |
| Snapshot/index cost | Existing DB owner, checkpoint reuse | Cold large-project freezes or rebuilding measurably interfere with saves; use bounded pre-staging without relaxing consistency. |

## 15. Delivery order

Build the context engine as a vertical extension to the existing feedback slice, not a prerequisite for basic writing. The companion plan defines reviewable packages.

The first working proof is: **save a chapter, ask about an old detail in an earlier chapter, retrieve exact evidence, inspect what the model received, receive a scoped suggestion, and apply it through the unchanged V3 contract.** It must survive a source revision, an unindexed save, a project switch, and a renderer restart.

Ship that deterministic evidence route before semantic extraction. Then add versioned digests and thin temporal/thread views. Add one qualified provider's read loop after the mock lifecycle and budget tests pass. Embeddings and higher-cost analysis follow measured failures, not the appeal of a more elaborate architecture diagram.

No delivery-date estimate is made: the current implementation state and team capacity have not been inspected. The supplied prior test counts remain reported historical results, not evidence that this extension is implemented.

## 16. Changes relative to the refined architecture

| Existing approach | New decision |
|---|---|
| One frozen context compiler per request | A frozen story snapshot plus one or more separately receipted packets under a bounded context session. |
| Exact references and local search | Keep them, add coverage-aware hierarchical navigation and evidence-bundle expansion. |
| Accepted summaries | Preserve their authority; add explicitly unreviewed, replaceable digests without confusing the two. |
| Small reviewed story records | Keep them as authority; add derived entity/time/thread views and tentative extraction only as aids. |
| Context from prior prose and notes | Add systematic older-story coverage, explicit conversation decisions, and persistent open-thread retrieval. |
| Model capabilities | Extend capability descriptors for read-loop support and token-accounting quality; do not assume generic compatibility. |
| Conservative stale-result policy | Unchanged. Better memory does not authorize automatic rebasing or Apply. |
| No compulsory planning or paid critique | Unchanged. Deterministic context works from ordinary prose; semantic maintenance is explicit/opt-in. |
| No graph/vector/agent platform initially | Unchanged. Relational structure and ordinary functions are sufficient until evaluation proves otherwise. |

The material tradeoff is more memory metadata and request construction work in exchange for better access to the wider story and an auditable account of omissions. The largest remaining uncertainty is not whether the database can store the novel; it is whether the chosen model and retrieval recipe consistently use the right evidence to produce better author-preferred prose. That is an explicit evaluation task.

## 17. Source register

Sources were inspected on **5 September 2026**. The links below identify primary papers or official documentation. Statements about tested systems are limited to those studies; architecture policies in this document are proposals.

| ID | Source | Relevant support / limit |
|---|---|---|
| S1 | OpenAI, [Conversation state — Managing the context window](https://developers.openai.com/api/docs/guides/conversation-state#managing-the-context-window), live documentation inspected 2026-09-05. | Input/output/reasoning accounting must be adapter-aware. No universal context length is assumed. |
| S2 | Liu et al., [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172v3), v3, 2023-11-20. | Position-sensitive use of long context in the evaluated tasks/models. Not a current-model benchmark. |
| S3 | Modarressi et al., [NoLiMa: Long-Context Evaluation Beyond Literal Matching](https://arxiv.org/abs/2502.05167v3), v3, 2025-07-09. | Long-context retrieval requiring latent associations is harder than simple literal needle matching in the tested models. |
| S4 | Sarthi et al., [RAPTOR: Recursive Abstractive Processing for Tree-Organized Retrieval](https://arxiv.org/abs/2401.18059v1), 2024-01-31. | Retrieval across abstraction levels. Its QA experiments do not establish fiction-writing benefits. |
| S5 | Saifullah et al., [Narrative World Model: Narratology-Grounded Writer Memory for Long-Form Fiction](https://arxiv.org/html/2607.05577v1), 2026-07-06, especially §§3, 7–8 and Appendix A. | Narrative-conditioned memory retrieval; generation remains unvalidated by the reported QA comparison, with explicit benchmark/ablation limitations. |
| S6 | SQLite, [FTS5 — The Trigram Tokenizer](https://www.sqlite.org/fts5.html#the_trigram_tokenizer), live documentation inspected 2026-09-05. | Substring capabilities and short-query limitation; qualify bundled SQLite compile features at build time. |
| S7 | OpenAI, [Function calling — The tool calling flow](https://developers.openai.com/api/docs/guides/function-calling#the-tool-calling-flow), live documentation inspected 2026-09-05. | Application-executed tools and subsequent model invocations. Capabilities must be qualified per model/adapter. |
| S8 | OpenAI, [Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching), live documentation inspected 2026-09-05. | Matching-prefix reuse and model-dependent behavior; no pricing, lifetime, or cache-hit guarantee is assumed. |
| S9 | Wu et al., [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813v2), v2, 2025-03-04. | Separate extraction, temporal reasoning, knowledge updates, and abstention evaluation; not a narrative-quality benchmark. |

**Local context:** `V3_ARCHITECTURE_REFINED.md` §§5–11; `V3_FIRST_SLICE_PLAN.md`; supplied `V3_RUST_REWRITE_PROPOSAL.md`; `NewResearch/Research1.txt` especially §§17.6–17.8; relevant craft guidance in `Research2.txt`. These attachments inform integration and requirements. No local V2/V3 source implementation, provider behavior, native application, or generated-novel benchmark was executed for this extension.
