# ADR 0013: Exact reviewed context for continuation

Status: core implementation complete for this slice. This extends author-only review in [ADR 0012](ADR_0012_AUTHOR_REVIEW.md); it exposes the exact freeze over Rust/IPC but does not enable a continuation button, live provider request, or ready export.

## One explicit basis

A reviewed continuation has two different sources: the working chapter where the author intends to continue, and the reviewed earlier story. The target remains working prose. Labeling the request reviewed must never imply that its unfinished target has been accepted.

The dedicated freeze command takes the current target head and a restricted-writing policy. It is available through the Rust/IPC boundary, not as a continuation control in the writing UI. Rust resolves every earlier active chapter in the exact `(position, document ID)` order. Each must have a current selected author-only bundle, an exact matching chapter head, the same disclosure policy, and a valid earlier prefix. A missing or changed source returns an explanation. No fallback to working drafts occurs. A first chapter has no earlier reviewed basis and is refused by this reviewed-freeze route; a future continuation action must choose the explicit Working basis instead.

This bounded command supports the target chapter's reader frontier only. It excludes later chapters, private notes, author-room chat, unclassified aliases, and guidance without a restricted grant. Character-specific knowledge and arbitrary historical bases remain unavailable. These restrictions do not prevent manual writing or ordinary working-draft discussion.

## Frozen evidence and subsequent changes

The snapshot stores an explicit reviewed basis manifest with exact bundle, document, revision, and head identities. It uses the existing immutable snapshot and source-pin storage. An absent optional field preserves the encoding of existing working snapshots; no duplicate manuscript or mutable facts store is introduced. Schema 15 adds a nullable reader-position column to source pins and establishes the minimum reader version for the new JSON field. Existing rows and their JSON stay intact, and migration takes a pre-upgrade backup. Schema-14 binaries must not open projects that can contain the new manifest.

Earlier descriptors are reviewed authority. The sole working descriptor is the continuation target. Pure eligibility verifies that this exception is limited to that exact target and purpose. Storage validation additionally resolves the recorded bundle provenance and revision identities; a client assertion or rehashed manifest cannot create authority.

Each reviewed source pin retains its resolved reader position, including the working target. Reading and backup validation compare the manifest against those immutable positions and require the frontier to equal the target position. They never reconstruct historical disclosure positions from today's chapter order. Older working snapshots do not require the new nullable position field.

The complete earlier prefix is the authority basis, not a claim that every chapter was supplied to the model. It remains in the frozen available-source manifest. The packet compiler may omit optional earlier prose within the request allowance and must report those omissions. The exact target and instruction remain mandatory. Original target prose does not acquire invented semantic dependency links merely because earlier chapters were reviewed.

Creation requires a current basis. Reading an older receipt validates its immutable recorded evidence without requiring that it still be today's selected basis. Any story-source change makes that snapshot stale for new use, while policy revocation stops further source reads. Independent copies preserve historical records but cannot use the copied operation namespace to start or replay work.

## Remaining dispatch work

The existing packet compiler still requires an explicit scope for continuation. This foundation does not relax any edit grant or add an Apply path. The forthcoming continuation action must distinguish a generated candidate from a manuscript mutation, freeze its exact intent and source manifest before dispatch, preserve partial/failed output, and require an explicit author action to incorporate prose. Its output placement and undo behavior need their own structural contract and native qualification.

An eventual basis selector must explain author-only review coverage and show the exact earlier sources. A failed reviewed request may offer a new working-draft request through an explicit author choice. It may not reuse the old operation while changing its basis. Optional accepted summaries, typed story records, blocking issues/exceptions, and temporal memory remain separate F2/C4/C5 work.
