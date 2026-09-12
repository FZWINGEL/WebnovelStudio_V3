# ADR 0021: Cross-chapter promise history

Status: implemented development slice. Current contract, native, and bounded
live evidence is recorded in [implementation status](IMPLEMENTATION_STATUS.md).
Broader live coverage and author-trial qualification remain pending. This
extends the reviewed evidence boundary in [ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md)
and the authenticated object history in [ADR 0019](ADR_0019_EVIDENCE_HISTORY.md).
It adds a narrow C5 promise view without creating a second mutable truth
database or claiming complete continuity.

## Author outcome

While reviewing a chapter, the author may record a promise observation against
an explicitly chosen project entity. The author chooses its phase (`setup`,
`payoff`, `cancelled`, or `unclear`), timing (`atPassage`, `earlier`, or
`unknown`), a short note, and its audience. The existing exact passage
capture remains the evidence boundary.

The identity is an opaque `StoryEntityRef`. Human labels help the chooser, but
matching labels never merge entities. A promise record describes what was
reviewed in one passage; it does not assert that the story has no other promise
evidence or that a payoff is the final narrative resolution.

## Reviewed record and persistence

`PromiseRecord` contains:

- an opaque record ID and `StoryEntityRef` promise identity;
- one phase: `setup`, `payoff`, `cancelled`, or `unclear`;
- one timing value: `atPassage`, `earlier`, or `unknown`;
- a nonblank note of at most 1,024 UTF-8 bytes with no controls;
- an audience of `authorRoom` (the default) or explicit `reader`; and
- one `EvidenceAnchor` containing exactly one block, UTF-16 offsets, grapheme
  boundaries, and the exact quotation/hash.

Schema 23 adds nullable `promises_json` and `promises_hash` to immutable review
stages and ready bundles. The fields remain attached to the existing immutable
review records; they do not form an independently editable facts database.
When the field is absent, the complete prior promise set is inherited only
after exact source and bundle revalidation. An explicit empty selection is
persisted as a nullable empty pair (`NULL`/`NULL`) through a new stage and
bundle. Existing possession records and legacy hashes remain unchanged when
the promise field is absent.

## Frozen context and history

A frozen context carries a separately authenticated `reviewedPromises` set.
Working AuthorRoom discussions may use the permitted set. Restricted reviewed
continuation receives only the reader-audience projection under the existing
policy and reviewed-prefix rules: author-room record identity, label, note, and
evidence are filtered before packet delivery. This record filter does not hide
source prose that is independently eligible under the request's source policy.
Promise records do not grant character knowledge or disclosure permission.

The context packet records delivered promise IDs, the original and projected
set hashes, and omissions. It uses the existing mandatory-source and budget
protocol. A context inspector distinguishes Available, Supplied, and
Historical material. Opening a source is read-only and adds no model input or
model call.

History groups records by opaque promise ID across chapters, preserving chapter
and source order, exact references, and quotations. It always reports
incomplete evidence. `hasRecordedPayoff` means only that a reviewed payoff
observation was included; it does not infer current resolution, fictional
chronology, a missing event, or a payoff from absence of a record.

The bounded live development check delivered an exact reviewed-promise packet
to the selected Luna profile, preserved the manuscript, and correctly stated
that missing payoff evidence does not prove non-occurrence. Context envelope
`webnovelstudio.context.packet.v2` now carries `PacketSource.displayName` for
supplied original sources and targets, plus `sourceDisplayName` on reviewed
promise sets even when a source body is omitted. These names are AuthorRoom
metadata only; restricted packets omit them. Stored v1 packets continue to be
validated with the v1 serializer, preserving their original budget selection
and bytes, while unknown envelope versions are refused.

The existing global source/context epoch fences new requests while leaving
historical evidence and prose unchanged. Policy revocation, copied or recovered
namespaces, stale bundles, and invalid source authority refuse new reads under
the existing boundaries.

## UX and boundaries

Story Review adds an optional promise editor alongside existing exact capture:
identity choice, phase, note, timing, audience, and reviewable source evidence.
The author must explicitly save the record; discussion text and assistant
suggestions do not become promises automatically.

This ADR does not define batch fact editing, knowledge or belief records,
embeddings, automatic payoff detection, arbitrary chronology inference, or the
C6 model lookup loop. Those remain separate designs and qualification gates.

## Remaining qualification

Deterministic checks cover exact anchor/hash validation, inheritance and
explicit clear, migration from schema 22 to 23, audience filtering, source and
namespace refusal, current-versus-historical reads, stable cross-chapter
ordering, incomplete evidence, packet hashes and omissions, and unchanged
possession/legacy records. Native development checks exercise review,
history/source opening without a model, restricted continuation, lost
acknowledgments, clearing, and reopening. The bounded live discussion cited
one old promise without inferring a payoff from missing evidence.

These checks do not establish reliable interpretation across long novels.
Broader live-provider and author trials must evaluate ambiguous promises,
conflicting observations, flashbacks, and incomplete extraction. In particular,
an absent payoff must not be presented as proof of cancellation, resolution,
or nonexistence. Exact executed checkpoints and their limits remain in
[implementation status](IMPLEMENTATION_STATUS.md).
