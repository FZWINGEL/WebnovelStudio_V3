# ADR 0020: Explicitly scoped structured suggestions

Status: implemented local development slice; local native qualification is
42/43 checks with the known OS clipboard case omitted. Hosted CI, broader
live-provider, and release qualification remain pending. This extends the
single-line passage contract in [ADR 0004](ADR_0004_PROPOSAL_APPLY.md) and the
append-only continuation contract in [ADR 0016](ADR_0016_STORY_CONTINUATION.md).
It defines the structured suggestion slice for complete block and whole-chapter
suggestions. It does not close the broader V3 editing or release gates.

## Author outcome

The author can ask for a suggestion for an explicit set of complete blocks or
for the complete chapter. The assistant returns a structured, editable prose
preview. The author can change that preview and then explicitly Apply or Reject
it through the existing proposal operation. Preparing or editing a preview does
not change the manuscript or establish canon.

The UI must make the scope choice visible. Use author-facing choices such as
**Selected paragraphs** and **Whole chapter**. A missing selection never means
`WholeDocument`, and the application must not silently broaden a text selection
to a paragraph range. The author explicitly chooses the paragraph scope; that
choice expands the selected text to complete blocks and shows the resulting
scope before the request is sent. Whole-chapter replacement is a separate,
explicit opt-in.

The initial block scope is contiguous complete document blocks. Richer arbitrary
multi-block partial edits, batch Apply, and manual rebind remain separate
designs. A selection that ends in the middle of a paragraph stays a passage
request until the author chooses **Selected paragraphs**.

## Decision

### Provider output

Structured revision requests use the versioned
`structured-proposal-output.v1` response contract. The output retains the
existing suggestion shape of a bounded list of titled candidates with an
explanation, but each candidate carries typed `blocks` rather than a
single-line replacement. The response contains one to three candidates. The
provider does not assign editor block IDs.

The provider-owned block grammar is deliberately small:

- `paragraph` with inline content;
- `heading` with an explicit level from 1 through 3 and inline content; and
- `sceneBreak`.

Inline content consists of `text` with optional `bold`, `italic`, or `link`
marks, and `hardBreak`. Text does not contain carriage returns or line feeds;
line breaks use `hardBreak`. Empty text nodes are rejected. The current bounded
grammar permits at most 128 replacement blocks, 100,000 UTF-16 units of
replacement content, and a 4,096-byte explanation. Malformed, unknown, or
unsupported output remains retained provider/discussion text without an
applicable candidate; there is no automatic repair generation.

The durable proposal kind distinguishes `Structured` from legacy `Passage` and
`Continuation`. Existing passage response bytes, single-line replacement
validation, continuation payloads, receipts, and hashes retain their existing
meaning. Structured output cannot acquire passage or append authority by adding
fields, and old payloads do not acquire structured fields during deserialization.

### Identity and preparation

The editor is the only owner of live document identity. After the author has
chosen a scope, JavaScript captures the immutable source snapshot, validates the
editor transaction shape, and allocates fresh block IDs for the replacement
blocks exactly once. It prepares the complete result snapshot, including the
unchanged prefix and suffix, and sends that snapshot plus the typed candidate to
Rust. An uncertain preparation acknowledgment retries the same prepared body
and IDs; it does not regenerate IDs or ask the provider again.

Rust independently validates the typed candidate and the complete prepared
snapshot. It checks canonical document structure, valid and unique replacement
IDs, freshness against the source IDs, exact block order and content, and the
selected scope. Every unselected block, boundary, block style, mark, scene
break, and identity remains protected by the shared scope validator. A provider
cannot smuggle executable editor steps or silently change surrounding prose.

### Preview and Apply

The preview is one editable rich-prose surface. It renders paragraphs,
headings, scene breaks, marks, and hard breaks as the author will experience
them; it does not expose raw JSON as the editing interface. Editing the preview
creates a new prepared version without a model call and retains the exact
structured block payload and prepared snapshot needed for retry.

Apply keeps the existing short editor barrier and durable transaction. The
mounted editor flushes and preflights the prepared result, then Rust checks the
current document head, source and policy fences, operation namespace, prepared
version, and receipt identity. One atomic operation records the new working
body, before/after revisions, author decision, and receipt. Only the confirmed
acknowledgment is displayed in the editor. Lost acknowledgments reconcile by
operation identity; they never replay an edit blindly. Undo, history, copied
records, stale-source refusal, policy revocation, and recovery retain the
existing proposal rules.

Applying a structured suggestion changes the working manuscript only. It does
not activate facts, summaries, guidance, evidence, or publication state.

## Boundaries and non-goals

- `Passage` remains the legacy one-line inline replacement path.
- `Continuation` remains append-only and keeps its existing wire contract.
- `Blocks` authorizes a complete contiguous block range; it does not authorize
  arbitrary character ranges across several blocks.
- `WholeDocument` is an internal scope name for the explicit whole-chapter UI
  action; it is never selected by an empty or missing selection.
- Batch Apply, overlapping suggestion sets, arbitrary partial multi-block
  editing, and manual rebinding are not part of this slice.
- Applying a suggestion does not infer or persist new narrative truth.

## Qualification plan

The deterministic fixture and local wrapper checks pass. The wrapper recorded
467 active Rust tests (442 core and 25 desktop, one existing ignored fixture),
301 frontend tests in 24 files, formatting, Clippy with `-D warnings`,
TypeScript, and Vite in `.local/structured-check.log`. The focused native
helper passed both structured journeys with no page errors. The full local
WebView2 diagnostic passed 42 of 43 checks with zero errors, omitting only the
known local OS clipboard case; evidence is `.local/structured-native.log` and
the report is `.local/native-other-results/report.json`. The executable is
SHA-256 `ca114dc73db5470332fd7b797c56380eb5244e303fa3595ca78c782eac685b15`,
30,628,864 bytes, built `2026-09-06T06:40:00.255Z`; the bundle is 718.66 KB
JavaScript / 37.16 KB CSS with the existing Vite chunk warning.

One bounded live structured request also passed with a retained proposal,
prepared version, decision, and provider result; evidence is
`.local/live-structured-qualification/qualification.json` and
`.local/structured-live.log`. Hosted CI and installed-release qualification are
still pending. Before calling the broader package complete, the implementation
must demonstrate, with deterministic fixtures and the native author journey:

1. legacy passage and continuation payloads remain byte-compatible;
2. malformed typed blocks, invalid heading levels, line breaks in text,
   duplicate marks, size limits, and unknown fields are rejected without a
   candidate;
3. JavaScript and Rust agree on the same prepared snapshot, including marks,
   hard breaks, headings, scene breaks, fresh IDs, and retry identity;
4. changed or protected surrounding blocks, stale heads, policy changes,
   copied history, lost acknowledgments, Apply, Reject, undo, and reload follow
   the existing fences; and
5. the UI requires an explicit paragraph or whole-chapter scope and presents a
   readable editable rich-prose preview.

This ADR records the implemented contract and its local evidence. It does not
claim hosted CI, broader live-provider, or installed-release qualification.
