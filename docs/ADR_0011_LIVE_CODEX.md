# Bounded Codex development path

**Status:** Implemented bounded desktop path; provider compatibility policy revision in progress, enabled only after an explicit installed-CLI compatibility check

**Date:** 2026-09-06

This decision records the first live-provider boundary for V3. It connects a
bounded Codex runner to the existing packet, discussion, and recovery contracts
without treating a successful native CLI experiment as full provider
qualification. The Windows desktop can dispatch this bounded path only after
the author explicitly checks the Codex connection; until that check succeeds,
Codex remains blocked and the local test model remains available. Because Codex
updates frequently, compatibility is checked against the installed CLI at
connection time. V3 does not pin a Codex version or executable hash; observed
identity is recorded with the request for diagnosis and provenance.

## Decision

Use one explicit, immutable `ProviderBinding` for a Codex request. Capture it
when the context packet is compiled, serialize the exact packet through stdin,
and persist the binding with the discussion run and terminal provider result.
The binding identifies the requested provider/model and requested traits; it
does not claim that the upstream process echoed or honored those traits.

The first bounded development profile is:

| Field | Value | Meaning |
| --- | --- | --- |
| Provider | `codex` | Native CLI provider identifier |
| Model | `gpt-5.6-luna` for all background work; picker-selected for author requests | Requested model |
| Reasoning | `xhigh` for all background work; picker-selected for author requests | Requested reasoning level |
| Service tier | Picker-selected; historical Codex evidence used `priority` (“Fast”) | Provider service-tier ID when supported |
| CLI profile | Installed CLI compatibility profile | Runtime compatibility check; no release-version or executable-hash pin |
| Input allowance | `24576` bytes | Application cap over the exact UTF-8 stdin packet |
| Reserved output/protocol | `0` / `0` | No model headroom claim; these fields are explicit non-capability values |
| Retained output | `65536` bytes | Application cap for validated assistant text |
| Accounting | `utf8-byte-count/codex-stdin-application-cap-v1` | Describes the local accounting method |

The input and output numbers are application limits. They are not a Codex
context window, output-token allowance, or provider billing limit. The tested
`model/list` response did not expose context or maximum-output capability
fields; locally cached values are deliberately not promoted into this
contract. The exact packet and binding structures live in
[`context/packet.rs`](../crates/core/src/context/packet.rs).

The compatibility-aware runtime identity field is a schema-25 reader-floor
change. Legacy 0.153.3 Max packets remain readable without that optional field,
and their serialized bytes and hashes stay unchanged. New receipts may record
the observed installed version and executable hash, but those observations do
not become a future launch pin.

## Request and dispatch boundary

`PrepareContext` and `StartDiscussion` accept an optional binding. When it is
present, packet compilation validates the selected supported development profile,
the model/trait values, the accounting label, and the byte allowance. The
serialized packet is bounded before dispatch and its immutable packet record
retains the request hash and options needed to reconstruct the same stdin
bytes. The live path leaves `max_output_tokens` empty because no upstream
token limit has been qualified; the existing mock serialization and receipt
hashes remain unchanged.

Live scoped `ProposeEdits` derives `proposal-output.v1` inside Rust. The
optional versioned contract is retained with the compiler request and adds
trusted JSON-only instructions to the frozen system message. Its bytes count
toward the same input cap and hash; the final author instruction is unchanged.
The shape is `{"suggestions":[{"title":"...","replacementText":"...","explanation":"..."}]}`,
with one to three alternatives or an empty array when no valid edit is
possible. Each replacement is limited to the selected quotation; explicit
deletion may use an empty replacement. The existing scope validator still
decides whether an author-prepared edit is acceptable. Generic renderer
preparation cannot select this response contract. Old mock and live packets
without it reconstruct their original messages and hashes.

The desktop worker passes the packet's exact serialized bytes to the
Windows process primitive through stdin. Prompt content is never placed in
arguments. The process boundary owns the executable, fixed arguments, working
directory, explicit environment policy, bounded pipes, and Job Object
containment. Its contract is documented in
[`WINDOWS_PROCESS_CONTRACT.md`](WINDOWS_PROCESS_CONTRACT.md). The Codex profile
constructor and recorded native evidence are in
[`CODEX_QUALIFICATION.md`](CODEX_QUALIFICATION.md).

The binding is a request snapshot, not a reference to the current global
model selector. A later Settings change cannot redirect an accepted run. A
`DiscussionRun` read returns both the immutable binding and any immutable
`ProviderResult`, so reopening the project shows what the run requested rather
than what the application currently prefers.

## Explicit desktop enablement gate

The desktop runtime does not enable Codex when the picker opens and does not
substitute another model when the check fails. `check_connection` clears the
session connection first, discovers the installed native Codex executable,
checks that its current version and supported launch surface are compatible,
records the observed executable identity, and confirms `login status`. The
resulting session connection is held in native runtime state and is required
before a live worker can start. The selected model, reasoning, and service
tier must also match the immutable bounded binding. No credential contents are
read into application state. Historical 0.153.3 runs remain qualification
evidence, not a current version requirement.

## Owned Stop and terminal outcomes

Stop is owned by the run's `RunOwner`. The worker registers the owner before
external dispatch, polls the local stop signal, and reports a typed terminal
result. The core applies the following precedence while settling the report:

Legacy mock completion and delivery commands reject live-bound runs with
`ProviderResultRequired`. Only typed provider settlement can confirm their
delivery or completion. Backup validation checks receipt/event/sequence and
packet binding, and rejects a completed live run whose receipt is missing.

| Worker report | Cleanup | Durable discussion result |
| --- | --- | --- |
| Completed, full stdin delivered, valid text | Settled | `Completed`, proposals may be retained |
| Any report after author Stop | Settled | `Stopped`, no proposals |
| Timeout, output limit, protocol/provider failure | Settled | `Failed`, no proposals |
| Any report | Unresolved | `Interrupted`, no proposals |

Author Stop therefore wins over a late successful provider response. An
unresolved process cleanup cannot become a completed story operation. A
non-completed report may contain zero or partial confirmed stdin; a completed
report must confirm delivery of the complete serialized packet. Assistant
output must retain the current durable prefix and remains capped at 64 KiB.

The core stores only sanitized diagnostics, never raw provider configuration,
credentials, or unbounded output. `effective_identity` is intentionally
optional and the current runner must leave it absent: the qualification
streams did not echo an effective model, reasoning level, or service tier.
Usage is optional raw numeric data. Unknown usage stays unknown rather than
being inferred from local byte counts.

## Atomic terminal persistence and replay fencing

`settle_provider_discussion` validates the run owner, packet binding, expected
sequence, terminal event ID, output prefix, stdin byte count, cleanup state,
usage counters, and sanitized error before one database transaction updates
the run. The transaction appends the terminal event/message and inserts the
immutable provider result in
[`013_provider_results.sql`](../crates/core/src/storage/013_provider_results.sql).
The result records the packet ID, terminal event ID, expected sequence,
binding, retained text, outcome, confirmed stdin bytes, optional usage,
cleanup state, sanitized error, and optional effective identity. Update and
delete triggers make that terminal receipt immutable.

An identical retry is idempotent. A retry with a different event ID, sequence,
binding, outcome, text, or cleanup state is a conflict; the core never silently
replays a different external result. The stored packet remains the source for
request identity, while the result is the source for the provider's accepted
terminal report.

## Local recovery

If the worker loses its acknowledgement after the provider may have finished,
the application reads the owner-scoped run and the immutable provider result
before attempting recovery. `read_discussion_run` does not acquire a renderer
lease or start a provider. A saved report can be retried only against the same
run, packet, event, expected sequence, and durable output prefix. The retry
settles the existing operation; it does not generate again or create a second
run. A changed report is rejected for explicit reconciliation.

Committed terminal receipts survive restart. A report whose local save failed
is retained in application memory for an explicit local retry; it does not
survive process exit. Reopening an unfinished run records interruption and
preserves its already saved partial output. Unknown process cleanup remains
explicit. External execution is not exactly-once, and no automatic provider
retry is implied.

## Qualification evidence

The first eight bounded native dispatches against the installed direct Codex
executable at version `0.153.3` established the boundary below. Subsequent
chapter-memory, continuation, structured-suggestion, and promise-discussion
experiments are recorded with their individual limits in
[`CODEX_QUALIFICATION.md`](CODEX_QUALIFICATION.md):

1. A restricted direct-profile success using managed native execution.
2. An isolated-home failure showing that an empty auth home cannot be assumed
   to contain the managed login.
3. A normal-home success using the existing managed login without reading or
   copying credentials.
4. A Rust-materialized profile success with the exact Luna/Max/priority
   request.
5. A `CodexStream` completed case with cleanup settled and exact stdin
   delivery (`135/135` bytes).
6. A `CodexStream` Stop case with cleanup settled and exact stdin delivery
   (`104/104` bytes), but no assistant delta or usage.
7. A native desktop selected-passage request after the explicit connection
   check, with settled cleanup and `2336` confirmed stdin bytes. It returned
   ordinary prose, so no proposal was retained and Apply was not attempted.
8. The next native request used the frozen `proposal-output.v1` format, retained
   one proposal, and applied it without changing the protected ending. A
   read-only continuation after a harness selector error verified same-data
   reopen, saved requested traits, and Used context. No generation was replayed.
   Confirmed stdin was 3360 bytes; reported input/output/reasoning usage was
   1375/188/144, cleanup settled, and effective identity remained unknown.

The recorded successful streams observed no tool events. That is observation
evidence, not a universal no-tools guarantee. The runner cases exercised the
Windows Job-contained process foundation. The native desktop case exercises the
current bounded discussion path after the installed-CLI compatibility gate, but this
ledger still does not establish a supported provider or full W8 qualification.

This evidence does not qualify provider support, token limits, output-token
limits, upstream cancellation or billing cessation, provider retries,
credential isolation, author-data isolation, effective identity reporting,
general structured-output reliability, or refusal/truncation behavior. It also does not establish
that every executable, app, plugin, MCP, extension, or dynamic tool surface is
disabled. The adapter fails closed on unexpected protocol requests
and retain partial output.

## Consequences and next gates

This boundary gives the UI and recovery code a durable model identity without
letting the global selector mutate an in-flight run. It gives the worker one
bounded stdin/output contract and gives persistence one atomic terminal
receipt. It also keeps provider uncertainty visible: missing usage and
effective identity remain null, and local byte caps are not presented as model
capabilities.

Before declaring a supported live provider, the project still needs the
compatibility-aware process/profile adapter, provider JSONL decoding and
refusal/tool handling, interruption and cleanup qualification, context-budget
evidence, credential-boundary review, and native/package gates. Until those
gates pass, `ProviderState` must expose Codex as ready only for the currently
checked compatible session and supported binding; otherwise it must remain
explicitly blocked/reference only while the local test model stays ready. The
same contract must cover the planned model-picker adapters and configurable
OpenAI-compatible endpoint APIs. No full W8 qualification claim is made by
this ADR.
