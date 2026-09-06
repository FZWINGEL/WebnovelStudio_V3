# ADR 0022 — Bounded story lookups

**Status:** C6 development implementation and qualification in progress, 6 September 2026. Executed evidence belongs in [implementation status](IMPLEMENTATION_STATUS.md); this contract alone does not establish provider or release support.

## Decision and scope

Add an explicit **Look up story details when needed** choice to Working, AuthorRoom, and Discuss conversations. It authorizes an initial model invocation and at most two further invocations. Each receives a newly compiled, durably saved packet from the same frozen story snapshot. The ordinary one-packet discussion path remains available with lookup disabled by default.

This first C6 cut supports local search and exact source reads. It does not enable restricted writing, continuation, state queries, provider filesystem tools, autonomous canon changes, or manuscript edits. A broader lookup-enabled writing route must separately prove its disclosure and edit-scope contract. The application is for English novels; romanized names and Unicode text remain supported source content.

The UI explains the possible three model calls before sending. Lookup authorization is part of the persisted composer and exact request identity, including uncertain acknowledgments and linked retries. Changing the composer to Suggest edits or Continue clears it. Restoring an old chat does not silently authorize new calls.

## Provider boundary

Use a strict application response protocol, `story-lookup.v1`. This is an application-executed JSON route, not a claim that the provider exposes qualified native function calling or that a persistent provider session owns story memory.

An invocation returns one of these shapes:

```json
{
  "schemaVersion": "story-lookup.v1",
  "kind": "needsContext",
  "reads": [
    {
      "id": "search-1",
      "kind": "search",
      "query": "brass key",
      "mode": "literal",
      "limit": 6
    }
  ]
}
```

```json
{
  "schemaVersion": "story-lookup.v1",
  "kind": "needsContext",
  "reads": [
    {
      "id": "passage-1",
      "kind": "read",
      "handle": "exact-frozen-source-handle",
      "blockIds": ["exact-saved-block-id"]
    }
  ]
}
```

```json
{
  "schemaVersion": "story-lookup.v1",
  "kind": "discussion",
  "text": "The answer, with uncertainty and evidence explained in English."
}
```

Omitting `blockIds` requests the complete source. A present array must contain at least one unique valid ID. A missing source or block produces an unavailable result, never a guessed quotation. Search modes are literal, lexical, and exact alias. Alias matches identify sources; they do not pretend the alias occurs in the prose.

Each response may request 1–8 reads, with IDs distinct throughout the discussion. Search limits are 1–20, queries at most 512 UTF-8 bytes, handles at most 256 bytes, and block lists at most 32 IDs. The protocol rejects unknown keys, duplicate JSON keys, malformed shapes, arbitrary commands, and oversized output. Ordinary prose cannot execute a lookup. Normal JSON whitespace and multiline final discussion text are allowed.

The Codex development route retains the selected model and traits, process
ownership, disabled external tools, cancellation, and exact-input checks.
Codex compatibility is checked against the installed CLI at connection time;
the route must not pin a frequently updated version or executable hash. The
observed executable identity may be retained as request evidence. Background
summary/memory jobs use GPT-5.6 Luna with xhigh reasoning; an
author-facing lookup follows the persistent model picker. Every expansion
starts a fresh invocation; it is never described as a provider-session
resume. Intermediate response JSON is buffered and retained as invocation
evidence. Only a validated final `discussion.text` becomes the assistant
answer. The same protocol is intended to support the planned configurable
OpenAI-compatible endpoint adapters once their capability and failure gates
are qualified.

## Authority and durable records

Schema 24 introduces separate lookup invocation, result, and read records plus the optional saved composer allowance. Schema 25's observed-provider-runtime receipt remains a historical compatibility boundary. The current project reader floor is schema 28 because source-title projection must be validated as part of the authorizing reader contract; this prevents older readers from misvalidating the new projection and raises no SQL table or migration requirement. Library schema 4 is unchanged. Existing discussion packets, legacy provider-result bytes, and historical 0.153.3 Max packets remain valid and preserve their serialized bytes/hashes.

The existing discussion run owns the operation, project, namespace, target and initial packet. Every lookup invocation records its ordinal, exact packet and snapshot, source/policy epochs, allowance, and dispatch state. A committed claim can authorize one external start. Reading or reconciling an already claimed invocation cannot authorize a duplicate start.

Immutable invocation results retain the exact raw response, requested provider binding, confirmed stdin delivery, reported usage, cleanup status and errors. Immutable read receipts retain the requested operation and the exact locally resolved result. Repeating a local receipt must compare the full identity and payload; reusing an event ID with different output is a conflict.

All local reads are resolved inside the existing Rust project owner. No model-provided path, project ID, current-editor body, or arbitrary source text can redirect them. Search and manual context inspection share the same frozen-source matcher and Unicode-to-UTF-16 mapping. If a source cannot be searched, the result reports an unavailable gap instead of claiming complete search coverage.

The root run continues to identify the initial packet. A completed assistant message identifies the final invocation's packet. Conversation reuse, reopening, transfer validation, and the context inspector must preserve this distinction. Copied historical records remain inspectable history and cannot grant a new operation in a recovered project.

## Packet compilation and budgets

Each packet contains the original instruction, mandatory target and constraints, plus the bounded lookup exchanges that led to that invocation. Lookup results are not adopted guidance, generated canon, or an expansion of editable scope. The compiler independently checks exact source identities, passages, block order, completeness, search ranges and snapshot coverage; storage additionally binds these values to application-executed reads.

### Source-title projection for child lookup packets

Child lookup packet input may carry an app-owned `sourceProjection`:

```ts
type LookupSourceProjection = {
  schemaVersion: "story-lookup-source.v1";
  sources: Array<{
    handle: string;
    source: SourceRef;
    displayName: string;
  }>;
};
```

The projection is optional metadata on `LookupPacketInput`; the provider
`story-lookup.v1` protocol is unchanged. Child lookup packets derive exact
display names from the frozen context only for sources returned by that
invocation's search or read, then de-duplicate them deterministically. The
compiler independently validates the complete handle, exact `SourceRef`, and
frozen display-name set, together with AuthorRoom eligibility. Projection
metadata counts against the exact UTF-8 input budget and is never silently
dropped. An initial `None` lookup source set and a historical `None` source set
are not reconstructed from current context; prior packet bytes and hashes
remain authoritative.

The compiler also reproduces search semantics from the validated frozen passages: the query, search mode, aliases, returned matches, and remaining-match flag must agree. Quoting a real passage is insufficient if it does not match the requested search.

Required target text, selected scope, explicit instruction, mandatory pins and lookup evidence are never silently clipped. If required material cannot fit, the chain stops with an explanation. Optional context continues to use deterministic full-text or layered packing with recorded omissions.

The initial application allowance is:

| Limit | Default / maximum |
|---|---:|
| Additional invocations | 2 |
| Total serialized input | 73,728 UTF-8 bytes |
| Total retained output | 196,608 UTF-8 bytes |
| Per-invocation input | 24,576 UTF-8 bytes |
| Per-invocation retained output | 65,536 UTF-8 bytes |

These are application byte caps, not tokenizer estimates, model context limits, reasoning reservations, or a billing guarantee. Input and output allowances must be checked before another external start, with actual byte accounting rather than SQLite character counts. Provider-reported usage remains separate; absent values stay unknown.

At the invocation limit the model must answer with available evidence and explain uncertainty. A further lookup request is retained as output but cannot start another call. No unbounded planning loop, automatic retry, or paid autosave analysis is added.

## Stop, changes and failure recovery

Before another claim or expansion, Rust checks the owning project and namespace, current source/policy basis, invocation ordinal, committed Stop state and remaining allowance. An edit anywhere in the story invalidates continued expansion of that frozen request, including new evidence in a previously unsearched chapter. It never deletes later prose.

Stop while a response is finishing must settle that response without beginning another invocation. Failed stream cleanup remains explicitly uncertain. A crash after claim without a settled receipt remains interrupted with an unknown external outcome; restarting the app does not replay a paid call.

If a local result write fails or its acknowledgment is uncertain, the worker retains the exact result for an explicit local save retry and dispatches no further call. That retry records the available result and ends any remaining expansion; it does not restart the generation loop. A malformed completed response becomes failed invocation evidence. A deterministic rejection of a report's identity or delivery claim must terminate with an unconfirmed outcome rather than offer an impossible repeated save.

The existing JavaScript-owned editor, Rust snapshot acceptance, autosave watermarks and atomic Apply protocol are unchanged.

## Inspection and qualification

The inspector separates the initial context, additional search/read evidence, available but unsupplied sources, and gaps. It resolves labels from the packet's frozen projection, then an exact snapshot descriptor for legacy evidence, and finally the saved handle. Navigation preserves the source identity; renaming a current chapter cannot relabel historical evidence. Exact quotations link to saved source revisions. A no-match result says that the searched evidence contained no match; it does not establish that an event never happened or that no later transfer exists.

Each saved model-call packet must be inspectable. Prepared or unconfirmed calls must not inherit a delivered label merely because an earlier invocation was delivered. The final answer opens its actual packet by default.

Local input delivery is separate from response success. A failed or stopped call can retain proof that its complete packet was written; a partial or missing input receipt cannot. The inspector uses each invocation's exact retained input receipt for this distinction.

Reopening seals unfinished lookup calls in the same transaction as the interrupted discussion: a claimed call becomes unknown and an unstarted prepared call becomes stopped. Backup validation binds child packets to the original discussion request and checks invocation results, terminal events, and assistant messages together. Historical unavailable reads retain their bounded gap explanation rather than being silently reinterpreted from current sources.

Qualification proceeds through independent gates:

1. Pure malformed-envelope, source-binding, Unicode range and mandatory-budget checks.
2. Durable claim, duplicate receipt, altered payload, Stop, source/policy change, budget exhaustion, crash, reopen and recovered-copy checks.
3. Native opt-in → search → read → final-answer inspection and persistence using synthetic English material and the local test model.
4. A bounded live Codex request that actually asks for missing evidence, receives it in a subsequent saved packet, and returns a final answer without changing the manuscript.
5. Separate retrieval, uncertainty, narrative and author-trial evaluation. One successful live lookup does not prove general recall, literary quality or semantic disclosure safety.

The source-projection slice passes local contracts and the native rename/reopen
journey. Current counts and hosted results belong in
[implementation status](IMPLEMENTATION_STATUS.md). The earlier bounded live
lookup experiment predates this title projection; broader provider/release
qualification remains open. The broader [Story Context design](V3_STORY_CONTEXT_SYSTEM.md)
and [C0–C6 plan](V3_STORY_CONTEXT_FIRST_SLICE.md) remain the target. State/
thread tools, restricted-writing lookup, model-specific token accounting, and
narrative evaluation remain later work.
