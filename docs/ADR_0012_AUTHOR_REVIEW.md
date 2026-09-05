# ADR 0012: Author review of exact chapter prose

Status: implemented development slice. This is the first part of F2, not the complete reviewed-story system; the author-review CI/native checkpoint passed, while broader native/release qualification remains pending.

The review tables were introduced in project schema 14; the current project schema is 15 and adds the reviewed-context source-pin fields described in [ADR 0013](ADR_0013_REVIEWED_CONTEXT.md). Library schema remains 2. This slice stages an immutable exact saved chapter and its selected earlier revisions, records an explicit author-only Mark decision, supports optional writing and restart resume, exposes changed-earlier status, and clears active review heads in recovered or duplicated copies. The adjacent F2-B core slice can freeze the exact reviewed prefix plus current working target over IPC, but does not add a continuation UI, live dispatch, typed accepted facts or summaries, or ready export.

## Author action

**Story review** opens beside the existing manuscript. An author can inspect the exact saved chapter and its earlier reviewed chapters, then explicitly choose **Mark this version reviewed**. Writing, discussion, saving, and working-draft export remain available without a review. No model runs during review, and the action does not publish the chapter.

This first part records author-only review coverage. It accepts no extracted facts, generated summaries, character knowledge, or model claims of continuity. An empty set of typed story records is intentional: the original prose is useful evidence on its own. The reviewed source resolver now exists in the F2-B core freeze; reviewed output placement, live dispatch, typed authority semantics, and ready export remain unavailable.

## Durable contract

Rust stages an immutable review against the exact saved chapter head and revision. The stage retains the complete ordered earlier chapter prefix, every earlier selected valid bundle, the disclosure policy, the current context source epoch, and the target's prior selected bundle. Earlier chapters must have valid selected reviews for this particular action. This requirement does not gate ordinary writing.

The frontend displays the staged revision as inert text using the same formatting projection as saved history. The live editor stays mounted. Typing after staging is allowed; it makes the staged review outdated. Marking reviewed briefly joins the document lifecycle guard and flushes pending saves before submitting the exact stage and expected head.

One core transaction validates source and basis freshness, creates the immutable author-only ready bundle, records its earlier basis members and author operation, advances the selected pointer, and records the effects on later reviews. It never writes the manuscript body. Idempotent replay uses the same logical operation and payload; a new renderer lease does not create a second author decision.

While the editor session remains mounted, an unknown acknowledgment is resolved using the same operation after project reconciliation. Hiding the review panel retains that pending payload. After a renderer or application restart, the newly attached session reads the durable current review status. A committed decision appears as reviewed; the newest saved stage that was never accepted offers **Resume saved review**. Neither path automatically repeats acceptance. If staging never committed before the crash, there is no saved stage to resume; the author can explicitly prepare a new one. This does not promise persistence of an unsent renderer intent.

Earlier chapters in a staged basis expand into read-only previews of their exact revision IDs and hashes. They do not resolve to newer working prose. The author may inspect those sources without leaving the staged target or changing the live editor.

## Current validity and historical evidence

Current validity checks the exact selected chapter head, ordered prefix, policy, and earlier selected bundles. A stage's source epoch prevents activating a review prepared before an intervening story change. That global epoch is not a permanent validity requirement for every older bundle: reviewing chapter two must not itself invalidate chapter one.

Changing a working chapter preserves its old bundle and later prose. Later current reviews become unavailable when an earlier required review diverges or is superseded. A changed selected bundle also records a conservative suffix review fence. The interface distinguishes changed prose from a changed earlier basis; neither label asserts that later prose is wrong. Reaffirming unchanged later prose creates a new decision against the current basis.

Independent recovered or duplicated projects retain historical review records but clear active review pointers. Copied operation namespaces cannot authorize new reviews. Transfer validation checks review manifests and source references before accepting a project.

## Remaining F2 work

Typed accepted rules, events, beliefs, knowledge, disclosures, promises, optional accepted summaries, explicit issue decisions/exceptions, and known dependency evidence remain separate additions. They must activate through their defined author actions and valid bundle membership. Generated observations and navigation digests remain non-authoritative. The reviewed-continuation core freeze is implemented over IPC with exact source resolution and historical manifest validation; a continuation UI, live dispatch, ready export, typed records, and historical-manifest selection remain separate qualification work.
