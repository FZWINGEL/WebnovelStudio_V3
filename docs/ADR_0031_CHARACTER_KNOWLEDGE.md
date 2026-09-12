# ADR 0031: Passage-backed character knowledge

Status: implemented development slice with local contract/native evidence in
[implementation status](IMPLEMENTATION_STATUS.md). Extends the existing F2
reviewed record boundary and C5 evidence views; it does not complete F2/C5,
installed-release qualification, or qualify a new provider route.

## Author outcome

During optional chapter review, the author can record what a character knows,
believes, suspects, rejects, explicitly does not know, or is uncertain about.
Each observation retains an exact passage in the reviewed chapter. Later
context distinguishes these attitudes instead of treating every mentioned
claim as established world truth. The context inspector opens the original
evidence and shows the character's recorded history.

## Record and authority contract

`KnowledgeRecord` contains `id`, `character: StoryEntityRef`,
`topic: StoryEntityRef`, `attitude`, `statement`, `timing`, `audience`, and
`evidence: EvidenceAnchor`. `attitude` is one of `knows`, `believes`, `suspects`,
`rejects`, `unaware`, or `unclear`. `timing` reuses `atPassage`, `earlier`, and
`unknown`. `statement` is required, nonblank, at most 1024 UTF-8 bytes, with no
control characters. Entity/record identities, labels, evidence ranges and
quotations use the existing reviewed-record limits. Each complete set has at
most 64 observations and 64 KiB canonical JSON.

Character and topic identities are opaque project-local choices. Equal labels
never merge identities. An existing character can be reused from reviewed
entity choices; topics have their own chooser. A `knows` observation is the
author's interpretation of that passage, not an independent proof of the
statement's truth. `unaware` requires affirmative evidence of unawareness;
missing records never create it. Conflicting statements remain inspectable.

`StageAuthorReview.knowledge` is an optional complete array: omission inherits
the selected bundle's entire set after exact-source validation; `[]` explicitly
clears it. Every changed form must be restaged. Acceptance commits knowledge in
the same existing ready-bundle transaction, advances the source epoch and
fences later review selections. Neither recording nor inspecting it changes
prose, makes a model call, or creates another editable truth store.

Schema 33 adds nullable `knowledge_json` and `knowledge_hash` to review stages
and ready bundles and raises the reader floor. Null/null is legacy absence.
Old requests, bundles, snapshots and packet bytes remain unchanged. Recovered
projects preserve historical observations but clear active reviewed authority.
Loading, staging, accepting and freezing authenticate the full record set,
hash, immutable source, project and operation namespace.

Stored-snapshot loading is the authority boundary for Working snapshots:
`validate_pins` compares the sidecar namespace and complete contents with the
immutable bundle. Pure packet/history functions validate source consistency
and disclosure but cannot authenticate a hand-built Working snapshot against a
database. Production commands load and authenticate it in Rust before calling
those functions; renderer-supplied manifests are never accepted as authority.

## Frozen delivery and history

`ReviewedKnowledgeSet` uses the existing promise-set envelope shape with
`KnowledgeRecord[]`. `FrozenContext.reviewedKnowledge` and matching receipt
coverage/omissions are optional, omitted when empty. The compiler validates
complete sets before choosing a budget branch, validates exact evidence
against source reads, and budgets delivered records with their quotations.
Only the delivered eligible record IDs are listed in coverage. The inspector
separates available observations from those supplied to the model.

Working AuthorRoom discussion/planning/story questions may use current,
selected, exact-source reviewed knowledge. Reviewed restricted continuation
receives only explicitly reader-disclosed observations from its frozen earlier
reviewed prefix. Private observations are removed before context-inspector
rendering, searchable display, model delivery and delivered-context budgeting.
Disclosure omissions retain the permitted source handle, bundle ID,
complete-set hash, reason and count for receipt reconciliation; they never
include hidden observation IDs, character/topic labels, statements or quotations.
Fictional earlier timing never bypasses a reader frontier. Original prose has
its own disclosure policy; these records cannot make an unsafe passage safe.

`reviewed_knowledge_history(access, snapshotId, characterId, topicId?)` is a
read-only authenticated query over that frozen manifest. It returns exact
eligible observations in reader/source order, their original evidence, label
variants and uncertainty flags. It always reports incomplete coverage. It does
not collapse them into a definitive current mental state, infer contradiction
from different wording, or equate paragraph order with fictional chronology.
Earlier/unknown timing, different recorded attitudes, disclosure limits and
no eligible observations are visible without leaking hidden record identities.
Historical queries retain original evidence; current source/policy changes
retain their existing stale/revoked behavior.

The authenticated local author renderer may receive the complete frozen
manifest, as it does for possession, promise and summary records. It is not a
restricted model principal: authors can inspect and edit their private story
material elsewhere in the app. Rust authenticates the complete persisted set;
provider packets and the inspector's restricted view receive the permitted
projection. Do not truncate the authenticated set in transit while retaining
its complete-set hash, or treat renderer inspection as model delivery.

This record does not grant a character permission to read a complete chapter.
Character-limited generation still needs independently qualified exact-source
grants. C6 currently supports search/read; a dedicated model knowledge lookup
and generated extraction remain separate follow-on integration, not implied
by an author-facing history query.

## Required evidence

- Schema migration, legacy absence/bytes, exact Set/inherit/Clear, stale anchors,
  idempotent acceptance, rollback and recovered-copy authority boundaries.
- Mixed private/reader records; future learning cannot become earlier knowledge;
  unknown/earlier timing and absent extraction never imply a known state.
- Complete-set and source authentication even when budget omits observations;
  exact packet/receipt retention and tamper refusal after canonical rehashing.
- Form/session races, saved-stage resumption, explicit restaging and acceptance,
  inspector delivered/available filtering and exact source navigation.
- Native synthetic author-review and packet-inspection journey; any live model
  result is recorded separately from deterministic retrieval correctness.
