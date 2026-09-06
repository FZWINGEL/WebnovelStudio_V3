# ADR 0015: Generated chapter memory in working discussions

Status: C4-B implemented development slice; local contracts and native diagnostic verified. Strict CI and broader qualification are recorded separately. This extends [chapter memory](ADR_0014_CHAPTER_MEMORY.md); executed checks belong in [implementation status](IMPLEMENTATION_STATUS.md).

## Author experience

Ordinary working-story discussions can reuse existing chapter memory without another model call or a configuration step. When full eligible prose fits, the request supplies that prose. When it does not fit, an existing compact summary can provide broader orientation while exact source revisions remain available for inspection. Refreshing memory remains an explicit, separate action. Saving or opening a discussion does not generate summaries.

The first integration supports working-basis author-room discussion, planning, and story questions. It excludes the current target, restricted writing, reviewed continuation, historical-basis requests, and memory analysis. These exclusions keep private context and reviewed authority boundaries intact while the generated-view contract is qualified. Richer recipes need their own complete dependency contracts.

## Frozen identity and retention

A `FrozenNavigationView` has its own view ID, original project and operation namespace, canonical candidate fingerprint, source epoch, disclosure policy, complete original-source dependencies, and generated payload. The fingerprint identifies generated JSON; it is never a manuscript body hash. Original source descriptors and generated views remain separate contract families.

When freezing an eligible request, Rust automatically selects at most 64 views, in stable source order: the latest valid current view for each eligible non-target chapter. It requires the same project and namespace, exact source revision, policy, and source epoch. A changed chapter or previously unsearched story source conservatively prevents old memory from entering a new snapshot. The full original chapter remains in the available source manifest.

Schema 17 adds immutable `snapshot_navigation_views` pins. The snapshot and pins commit together. Historical reads validate the frozen payload against the immutable view and its original source dependencies, rather than reconstructing memory from current heads. Missing, extra, mismatched, or foreign pins are refused. Recovery retains historical evidence but gives the recovered project an independent namespace; copied views cannot silently become its current memory. Disclosure revocation still blocks request-facing reads.

Existing schema-16 memory rows are not rewritten. Old snapshot and packet JSON omit empty new fields, preserving their serialized request input and receipts. Migration takes the normal pre-upgrade database backup.

## Packet priority and coverage

The compiler validates every frozen view's fingerprint, exact dependencies, and source quotations before budgeting. The full-text path keeps its existing input. If all prose fits, generated alternatives are omitted and the receipt explains that original text was supplied instead.

The layered path preserves the exact target, author instruction, required source pins, adopted guidance, and approved brief. It keeps the existing complete recent-exchange priority. It then tries whole compact generated views in frozen source order, followed by the existing whole-block fallback for sources not represented by a delivered view. It never trims summary items or mandatory prose, and does not supply both a generated view and duplicate original text for that source.

Generated payloads have a separate `derivedViews` envelope with explicit unreviewed status and source references. Receipt view references record actual supplied coverage separately from original-text source handles. Available but omitted views record `originalTextIncluded`, `budget`, or `notSmaller`. Input accounting uses exact serialized bytes under the existing application allowance; it is not a newly qualified model token budget.

## Inspection and limits

The context inspector counts and expands generated summaries separately under Used/Prepared and Available. It shows exact frozen item text, uncertainty, and quotations, and opens original evidence through the saved snapshot source handle. A missing exact dependency never falls back to the current editor. Policy failures and project changes clear the previous contents.

A valid quotation proves a text match, not a correct interpretation. Generated summaries remain unreviewed navigation aids, never author guidance, accepted narrative summaries, facts, or manuscript edits. No save, Apply, authority, dispatch, or external usage protocol changes. Arc/story summaries, temporal state, bounded model lookup, and narrative evaluation remain later work.
