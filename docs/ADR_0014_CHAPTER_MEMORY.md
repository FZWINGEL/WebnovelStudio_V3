# ADR 0014: Source-linked chapter navigation memory

Status: C4-A implemented development slice; local, native diagnostic, and one live-provider result verified. Strict CI and broader qualification remain separate. Executed checks and native/provider qualification belong in [implementation status](IMPLEMENTATION_STATUS.md). This is the first generated-memory cut of the [Story Context design](V3_STORY_CONTEXT_SYSTEM.md), not a claim that broader derived-view packet integration, richer memory, or narrative understanding is complete.

## Author action and permitted input

**Refresh story memory** explicitly requests a short navigation aid for one saved chapter, using the selected local mock or qualified GPT-5.6-Luna/Max/Fast profile. Opening the panel, typing, saving, autosave, switching projects, and reading an earlier result do not request generation or make a paid call. The author can keep writing while analysis runs. Closing the panel leaves the job running; Stop is a separate action.

The controller briefly flushes the manuscript and captures its exact head. Rust checks the head, project, operation namespace, and writer lease. In one local transaction it freezes the chapter revision and persists the complete serialized request packet and job. The analysis recipe contains exactly that full chapter revision. It excludes other chapters, notes, aliases, chat, adopted guidance, and previously generated digests. It has no edit grant. This narrow recipe makes the first dependency contract testable without introducing recursive summaries.

The full chapter is mandatory. An oversized chapter returns a budget error instead of silently dropping passages. Current budgets are application UTF-8 byte allowances, not qualified model token windows. The selected live profile uses the existing bounded Codex integration; the local mock remains available offline. The offline test model produces labelled, evenly distributed extracts; those extracts do not establish summarization quality.

## Three durable records

Schema 16 adds a job, an immutable terminal result, and an immutable generated view with an explicit source relation; library model preferences remain schema 2. It also establishes the minimum reader version for the new memory-analysis packet contract. Migration preserves earlier snapshot JSON and takes the normal pre-upgrade backup.

- The job binds the exact request, selected producer, source revision, packet, source epoch, disclosure policy, and project namespace. Its only mutable fields describe local execution status.
- The terminal result retains bounded provider output, validation outcome, reported usage, local stdin delivery, and process cleanup. Saving this result is independent of installing a view.
- A view retains the validated candidate and its exact source dependency. It is an unreviewed navigation aid. It is never an accepted summary, canon fact, author instruction, manuscript revision, or additional working body.

Installing a generated aid does not advance the story-source epoch. This avoids making ordinary analysis invalidate the source it just read. Editing story material still advances the epoch and conservatively marks previous analysis as changed; stale views remain visibly flagged and cannot silently become current. Source change and disclosure revocation are different: stale source evidence may remain inspectable, whereas revoked material and diagnostics that might quote it are redacted from request-facing reads.

## Validation and meaning

The `navigation-digest.v1` response is a bounded JSON object containing its exact `SourceRef` and 1–16 items. Every item has 1–4 evidence quotations. Rust independently verifies the canonical source body and passage projection, then checks each block ID, UTF-16 range, surrogate boundary, and exact quoted text. Unknown fields and mismatched identities are refused. The retained invalid output can explain a failed analysis, but cannot become an installed view.

Correct citations do not prove that an interpretation is true or complete. Items can distinguish uncertainty and are displayed with links back to the saved chapter. The original manuscript remains retained even where the digest omitted a detail. Typed state, knowledge, promises, and accepted summaries require their own later contracts and author review.

## Dispatch, Stop, and recovery

Claiming a queued job durably records dispatch before external submission. Only the caller receiving the first successful claim may launch a worker. Replaying the same operation or reading a running job cannot authorize another submission. An uncertain claim or a process crash is treated conservatively; an external outcome may be unknown.

If the desktop cannot confirm the local claim, it retains that uncertainty without launching a worker. An explicit local check marks the owned job interrupted, including when the claim actually committed. The same job cannot be dispatched again. A fresh refresh is a new explicit author action. A live completed result requires confirmed process cleanup as well as exact local stdin delivery; unresolved cleanup remains an interrupted outcome even when Stop was requested.

The worker holds its own `ProjectSession` and exact owner. Changing the visible project cannot redirect its result. Stop is persisted before signaling the owned provider process. A stopped or failed result cannot install a current generated view. Cleanup state is retained separately from the output outcome. Historical recovery and copied namespaces retain evidence for inspection but cannot claim, dispatch, or install authority for the source project.

The worker first persists the terminal output, then independently installs a valid completed candidate. Failed local persistence retains the exact terminal event in the desktop process for an explicit save retry. That retry completes or reconciles local writes only; it never starts another model request. If the application itself exits, an unfinished job becomes interrupted on reopen and is not automatically replayed. A result already saved before an interrupted install remains available for local installation. Explicit Archive removes the project session; a same-process reopen may mark the job interrupted while retaining a terminal payload from a failed local write. Local retry then retains that exact terminal as interrupted history and never installs or resumes it, and does not discard it.

Backups validate immutable historical identities and dependencies without substituting today's document versions. Recovery and duplication create independent project namespaces; recovery checks retain the recovered project's original project identity as well as its namespace, while copied jobs cannot dispatch or adopt results on behalf of the original project. Replaying the exact operation is a no-op/refusal and never redispatches the producer.

## Subsequent context use

This first cut exposes generated memory for author inspection. Until a separate derived-view reference and packet contract is implemented and qualified, these views are not supplied to later model requests. A generated view must never masquerade as a manuscript `SourceRef` or alter the bytes denoted by a revision hash.

The next C4 increment must freeze the chosen view identity, content fingerprint, complete source dependencies, disclosure policy, and coverage label. It must preserve exact task text, distinguish digest coverage from verbatim prose, omit stale or ineligible views, and retain the exact view for already frozen requests. Arc/story summaries and incremental rebuilds follow only after this contract works for chapter views. C5 temporal relationships and C6 bounded read-only model lookup remain separate work.
