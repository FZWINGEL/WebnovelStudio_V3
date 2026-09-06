# ADR 0019: Cross-chapter reviewed evidence history

Status: CI-qualified development slice. This follows the schema-21
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

## Qualification and remaining work

The implementation includes a read-only entity-choice projection from current
selected bundles, authenticated object-ID history over `FrozenContext`,
disclosure filtering before grouping or serialization, preserved reviewed-prefix
and record-array order, uncertainty markers, and refusal/currentness behavior.
The full local wrapper passes 459 active Rust tests and 275 frontend tests,
plus formatting, Clippy, TypeScript, and build checks. Evidence is
`.local/evidence-history-check.log`.

The local native diagnostic passes 40/41 checks with zero errors, omitting only
the known local OS clipboard case. Evidence is `.local/evidence-history-native.log`
and `.local/native-other-results/report.json` (`2026-09-06T05:40:09.731Z`,
WebView2 `152.0.4191.62`). [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823) passes both contract jobs and all 41 strict native checks.
The retained native report is `.local/ci-34014694823/report.json`, dated
`2026-09-06T05:56:35.15Z`, on WebView2 `151.0.4129.101`, for source
`284625b6576540939f3dabb065f1e9320d0bb01e`.

Broader author and narrative quality evaluation remain open. The separate measurements in
`.local/reviewed-evidence-freeze-benchmark/result-v3.json` use two samples at
50, 100, and 200 chapters: catalog lookup is about 3.9, 10.2–11.1, and
32.6–33.1 ms, while history lookup is about 42.1–42.5, 225.7–229.7, and
1,437–1,486.7 ms. Source-bound freeze does not remove historical snapshot
revalidation cost; these fixtures do not establish whole-request speed,
large-novel readiness, or an exact asymptotic bound. Batch immutable snapshot
validation is the next priority before scaling richer history or other C5
views, while preserving policy, namespace, packet-receipt, and bundle-
authenticity checks.

This ADR records the implemented contract while qualification continues. It
does not change the existing save, review, Apply, packet, or model-provider
protocols.
