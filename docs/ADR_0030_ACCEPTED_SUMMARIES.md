# ADR 0030: Optional accepted narrative chapter summaries

Status: implemented development contract; executed qualification is recorded in
[implementation status](IMPLEMENTATION_STATUS.md). Full F2 and narrative-quality
qualification remain open.

## Author experience

Story review can include an optional narrative summary. The author can write it,
edit an existing summary, explicitly clear it, or copy current generated story
memory as editable starting text. Copying does not stage, accept, or regenerate
anything. Saving a stage preserves the draft for resumption; **Mark this version
reviewed** activates it with the reviewed chapter. Prose-only review remains
available. Summary storage, editing, copying, and acceptance make no model call.
Any explicitly requested story-memory generation retains the separate
GPT-5.6 Luna/xhigh maintenance setting.

The first summary document contract is plain text, limited to 16 KiB of UTF-8,
with nonblank content and only newline/tab control characters. It has an opaque
immutable revision ID, text, explicit audience, an exact chapter SourceRef, and
the complete earlier reviewed prefix. It does not create another working
manuscript or a disposable generated-memory view.

Author room is the default audience. Reader-approved summaries require an
explicit author choice. This choice is an author assertion, not a guarantee
that the model cannot infer a secret from legitimate evidence.

## Persistence and authority

Schema 32 adds nullable canonical summary JSON and hash columns to immutable
review stages and ready bundles. Legacy absent fields remain absent from
request payloads, frozen snapshots, and packet receipts. The reader floor
prevents older executables from opening a project with these contracts.

Stage requests support an omitted summary, explicit Set, or explicit Clear.
Omission inherits a previous summary only when its exact source revision and
complete earlier prefix still match. Changed source or basis requires explicit
reaffirmation or clearing; the application never silently relabels an old
summary as current. Set creates a new immutable summary revision. Mark copies
the staged payload in the existing atomic review transaction, alongside the
selected bundle and receipt. Failed writes preserve the previous selection.

Canonical payloads, fingerprints, source/prefix bindings, and stage-to-bundle
equality are checked on read and transfer validation. Historical bundles remain
inspectable after source changes. Independent recovery preserves them as
history while clearing active review pointers, as for other reviewed material.

## Frozen context and delivery

Accepted summaries use a separate authenticated sidecar. Working author-room
requests may freeze current selected chapter summaries. Reviewed continuation
may freeze summaries only from the earlier reviewed prefix; the working target
does not contribute its own accepted summary. Source and dependency changes
still invalidate current eligibility through the existing review and epoch
rules. Snapshot reads authenticate immutable summary content against its
recorded bundle, without reconstructing it from current manuscript text.
The author's immutable audit snapshot retains complete accepted payloads for
authentication. Reader restrictions govern the provider projection and the
restricted inspector display; they do not deny the author access to their own
private story material in the desktop process.

The compiler validates exact source reads and dependency closure before any
budget branch. Full-text packets supply original prose and record that the
summary was omitted because the original was included. When full text does not
fit, complete accepted summaries receive priority over replaceable generated
digests in stable source order. They are used only when smaller than the
original representation. The target, pinned sources, explicit instructions,
and edit scope remain mandatory and are never replaced by a summary.

A delivered summary replaces optional prose/digests from that chapter in this
packet. The exact original chapter remains available for source inspection and
authorized retrieval. No summary is truncated to fit. The packet envelope
labels it `reviewedAccepted` / `narrativeSummary`; generated navigation remains
`unreviewedGenerated` / `digest`. Receipts distinguish supplied summary revisions
from original-source coverage, with explicit budget, disclosure, original-text,
and size omissions. Restricted delivery excludes private summary text and does
not serialize dependency titles from author-facing review metadata.

The inspector shows accepted narrative summaries separately from generated
chapter memory and opens their exact frozen original source. Availability,
actual delivery, and current freshness remain separate claims.

## Deliberate limits

This contract does not add automatic semantic acceptance, paid autosave
analysis, multi-chapter/arc summary generation, complete character knowledge,
or a claim of narrative correctness. Generated navigation remains disposable;
accepted summaries remain retained author decisions. Rich-text summary editing
and broader F2 records/issue decisions can extend this boundary later without
turning summaries into a second manuscript.
