# ADR 0018: Passage-backed author-reviewed story details

Status: CI-qualified development slice. This extends F2 review and provides the
evidence foundation for C5.
Full temporal reasoning, knowledge/belief records, rules, threads, and accepted
summaries remain later work.

## Author outcome

While reviewing a chapter, the author can optionally record possession evidence
from a selected passage: an object, its known holder (or unknown), and whether
the passage describes that scene, an earlier time, or unclear timing. These are
reviewed observations with exact evidence. They do not claim a complete record
of transfers or a definite present owner. No model call or manuscript edit occurs.

The first record grammar is `PossessionRecord`:

- `id`: stable record ID;
- `object`: `StoryEntityRef { id, label }`;
- `holder`: optional `StoryEntityRef`, with null meaning unknown;
- `timing`: `atPassage`, `earlier`, or `unknown`;
- `audience`: `authorRoom` or explicitly author-confirmed `reader`;
- `evidence`: `EvidenceAnchor { blockId, fromUtf16, toUtf16, quote, quoteHash }`.

Entity IDs are project-local opaque keys retained behind human labels. Choosing
an existing entity reuses its identity; typing the same name does not merge two
entities. Labels and source validation cannot prove semantic entailment. No free
form private annotation is smuggled into a restricted writing request. New
records default to author-room use; reader disclosure is an explicit author choice.

## Immutable review boundary

`StageAuthorReview.records` is an optional complete record array. An omitted
array inherits the selected prior bundle's complete set, which is revalidated
against the new target; an explicit empty array removes all records through a
new review. Invalid inherited anchors refuse staging and require explicit
removal or reselection. They are never silently discarded.

The stage pins the current exact target revision and records. The author reads
that stage and confirms `MarkReady(stageId)`. Records cannot be edited beneath
an existing stage or bundle. Changed detail forms require a new stage before
confirmation. Mark stores the complete set with the new bundle in the existing
transaction that replaces the selected head, fences later review selections,
and advances the source epoch. Prose coverage remains `authorOnly`.

Schema 21 adds nullable canonical `records_json` and `records_hash` columns to
review stages and bundles. Null/null represents the legacy empty set. A
nonempty array is bounded to 64 records and 64 KiB canonical UTF-8; IDs are unique,
labels are nonblank and at most 160 UTF-8 bytes, quotations are nonempty and at
most 4096 UTF-8 bytes. The hash covers the entire canonical array including order.
Each anchor is within one text-bearing block of the exact staged revision and
must match UTF-16 boundaries, quotation, and SHA-256. History validation checks
the body and complete record set, including every influential source.

There is no separately editable current-facts table. Read-only current views
resolve records from valid selected bundles; historical reads retain their old
set. Recovered copies preserve historical records and clear selected authority.
Old empty-set requests, stages, bundles, snapshots, and receipts retain their
serialized bytes. A schema-21 reader floor prevents an old application ignoring
nonempty reviewed evidence. Truthful old-schema migration fixtures remove the
new columns before downgrade.

## Frozen requests and delivery

A frozen `ReviewedEvidenceSet` binds project/operation namespace, bundle ID,
record-set hash, exact source reference/handle, and the complete immutable array.
It is a separate optional envelope in frozen context and packet coverage, never
a fabricated prose revision or generated digest. Rust selects and authenticates
it; client labels cannot grant authority. Old snapshots without sets remain
unchanged and cannot acquire records from a later bundle.

The complete set retained in a frozen snapshot is authentication provenance,
not a claim that every record is permitted for the request. For restricted
writing, Rust derives a reader-only projection before budgeting or building
model messages. The packet binds both the complete set fingerprint and the
eligible projection fingerprint; delivery receipts name only delivered eligible
record IDs. Disclosure omissions expose a count, never private record IDs or
labels. The inspector applies the same audience filter to its Available view.
The internal complete array cannot be offered through a model lookup as if it
were permitted evidence. Author-room requests may use the complete set.

Working author-room discussion may use records only from currently valid
selected bundles whose exact source appears in the frozen permitted sources.
Reviewed continuation may use reader-approved records only from its exact
earlier reviewed prefix. The target remains working prose. Existing reader
frontier, character grants, namespace, policy, and source freshness checks still
apply. Restricted working edits, private/future material, generated observations,
and memory analysis cannot obtain reviewed-record authority by relabeling data.

The compiler budgets records and exact quoted evidence together with source
context. It records exactly which records were delivered and why others were
omitted. Target, author instruction, edit scope, and mandatory constraints remain
protected. The inspector separates reviewed details from original prose and
generated navigation. It must not claim complete state extraction or possession
certainty from missing records. Reading never expands edit permission.

## Implementation and qualification

The schema-21 implementation covers the passage-backed possession-record
grammar, canonical validation and hashing, immutable stage/bundle persistence,
inheritance and explicit-clear semantics, historical authentication, frozen
context projection, reader-only restricted delivery, and context inspection.
The full local check is green with 446 active Rust tests (422 core and 24
desktop), one pre-existing ignored fixture, and 271 frontend tests in 22 files.
The final native WebView2 diagnostic passes 39 of 40 checks with zero errors,
omitting only the known local OS clipboard case. Its report is
`.local/native-other-results/report.json`, dated `2026-09-06T04:55:01.976Z`,
on WebView2 `152.0.4191.62`; the final executable SHA-256 is
`55fcc9784d8d5701f94769670f8edeba8083a3df12721a2ea0a7311fada3c564`, with
29,740,032 bytes, built `2026-09-06T04:53:59.7150787Z`. The final bundle is
701.85 KB JavaScript and 34.89 KB CSS with the existing Vite chunk warning.
Hosted CI 34012813796 passes all three jobs—Windows contracts, Ubuntu
contracts, and Windows native—with all 40 checks and no errors. Its report is
`.local/ci-34012813796/report.json`, dated `2026-09-06T05:08:11.66Z`, on
WebView2 `151.0.4129.101`; the source checkpoint is
`a8d73c36a3989e390682cf855f6eea071e58c91b`. Review actions call no model; this
qualification used the local test model, and no new live generation was run.

1. Pure record validation and canonical hashing; malformed/duplicate/Unicode
   anchors, unknown holders, and identity limits.
2. Stage/bundle persistence, full-set replacement, current versus historical
   reads, migration/backup, exact retry, and stale/fence guards.
3. Optional author-facing review forms, selected-passage capture, saved-stage
   resumption, explicit record removal/reselection, and readable evidence.
4. Frozen request authentication, bounded packet delivery, disclosure exclusion,
   separate context inspection, and old-snapshot byte stability.
5. Native author review -> add evidence -> restage -> explicit acceptance ->
   inspect delivery -> change/review -> retain old evidence and fence current
   dependencies, plus independent copy/recovery and lost-acknowledgment checks.

The package is complete only with the usable author flow and request inspection,
not just storage contracts. No live-generation quality or full C5 completion is
implied by deterministic or native checks. The complete V3 goal remains open.
