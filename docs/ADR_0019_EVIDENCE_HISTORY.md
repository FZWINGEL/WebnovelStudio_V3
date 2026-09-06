# ADR 0019: Cross-chapter reviewed evidence history

Status: implemented; qualification in progress. This follows the schema-21
passage-backed evidence boundary in [ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md)
and defines the next narrow step toward C5 state views. It does not establish
complete continuity, current ownership, canon, or large-book performance.

## Author outcome

When an author reviews a chapter, the entity chooser can reuse an
explicit project entity from a currently valid selected review set in
another chapter. The chooser shows the human label and the first chapter
context available for that entity, so reuse is an intentional identity choice.
The author can create a new entity when the existing entity is not the same
thing.

This is a project-wide author-room identity chooser. Reusing a label and ID
does not transfer that entity's facts, timing, or disclosure permission. A new
detail still needs its own exact passage and defaults to author-room use.

Matching labels are only search and display text. Typing the same name never
merges entities. The reused value is the existing opaque project-local entity
ID, not a new record inferred from spelling. There is no separately editable
facts database: reviewed evidence remains attached to immutable review bundles
and exact source passages.

Only entities from currently valid selected review sets are offered for reuse.
Stale bundles, revoked policies, copied or recovered project authority, and
records from another operation namespace are excluded. An unavailable earlier
entity must not be silently replaced with a same-label entity.

## Evidence-history query

The first C5 query is an explicit object-history request over an authenticated
`FrozenContext`, identified by the entity's opaque `object.id`. It reads the
reviewed evidence sets already authenticated into that frozen context and
returns an ordered evidence history for that object.

Before grouping, labels, or counts are computed, Rust filters each record using
the request's disclosure policy and permitted source set. Restricted requests
therefore see only reader-audience records; author-room requests may see the
records permitted by their author-room policy. A snapshot-wide
`disclosureLimited` flag may indicate that filtering occurred, but private
records must not reveal object-specific IDs, labels, quotations, or counts.

The result preserves:

- chapter order from the immutable reviewed prefix, or numeric reader position
  with document ID as the tie-breaker, and record-array order within a chapter;
- the recorded timing (`atPassage`, `earlier`, or `unknown`);
- unknown holders;
- the exact evidence reference needed to inspect the source; and
- an incomplete evidence state, because manual observations are never
  exhaustive.

The timing value records how the author described the passage. It does not
assert fictional chronology.

The query reports observations. It must not choose a definite current holder,
infer an unrecorded transfer, fill a missing interval, or convert absence of a
record into proof that possession continued. Later relationship, knowledge,
thread, rule, and summary views remain separate designs.

## Snapshot and delivery boundaries

The query is evaluated against the authenticated frozen context and its source
versions. `load_snapshot` authenticates the original bundles and current
policy/namespace. If a source has changed, the historical snapshot remains
readable and is marked `current=false`; a current-source request must not
silently substitute newer prose. Policy revocation or a copied/recovered
namespace refuses the read rather than returning an unverified history.

Opening evidence history reads existing retained material. It does not add new
model context, trigger a model call, or create a paid request. If a prior model
request used the history, its delivered material remains represented by that
request's packet receipt. A later history-panel read does not rewrite that
receipt or imply that the model saw the newly opened evidence.

## Planned implementation and qualification

1. Define a read-only entity-choice projection from currently valid selected
   bundles, including human label and first-chapter context.
2. Add authenticated object-ID history over `FrozenContext`; apply disclosure
   filtering before grouping, labels, counts, or UI serialization.
3. Preserve reviewed-prefix chapter order, within-chapter record-array order,
   timing, unknown holders, incomplete evidence, exact source references, and
   refusal reasons for revoked or namespace-mismatched inputs; return
   `current=false` for authenticated historical source changes.
4. Add pure and storage tests for same-label distinct entities, explicit reuse,
   private-record exclusion, historical source changes, policy revocation,
   copied authority, unknown holders, and no-inferred-transfer behavior.
5. Add native inspection and current-review validation before treating the
   projection as a usable C5 surface. Measure repeated-prefix work on synthetic
   chapter counts separately; no benchmark establishes large-novel readiness.

This ADR records the implemented contract while qualification continues. It
does not change the existing save, review, Apply, packet, or model-provider
protocols.
