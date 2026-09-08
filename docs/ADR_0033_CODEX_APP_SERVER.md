# ADR 0033 — Optional persistent Codex transport

**Status:** Implemented as an optional development transport. Exec remains the
default until the parity and release qualification gates below pass.

**Date:** 8 September 2026. The source baseline before implementation was
`9fb9c7e64dfb8433d56832975d7c03625dde9705`; the optional transport is included
with this ADR. The installed no-generation check and isolated
native Settings and mock Workshop checks pass. A live Workshop request also
passes generation, validation, persistence, and cleanup. See the [qualification record](APP_SERVER_QUALIFICATION.md)
for the preserved failures and exact evidence boundaries.

## Decision

The application now has a small Rust client for the installed Codex app-server
over local stdio, behind the existing provider boundary. It reuses an
application-owned server while starting a fresh, isolated ephemeral provider
conversation for each request. Rust still owns story context, request
persistence, response validation, and explicit author Apply.

The supplied architectural assessment reviewed `9cd51d5`. Its separation of
transport from story authority remains applicable to the newer source inspected
here. The transport implementation follows that boundary; the assessment and
the separate latency research remain supporting evidence rather than provider
qualification.

The intended application path is:

```text
React / Tiptap → Tauri commands → Rust story core and durable request
                                      ↓
                               Provider boundary
                               ├─ Codex: owned app-server over stdio
                               ├─ existing Claude integration
                               └─ existing OpenAI-compatible integration
                                      ↓
                         Validate → persist → author reviews / applies
```

No TypeScript or Python sidecar is needed for this direction. Official SDK
documentation describes a server-side Node library and a Python library that
controls app-server; direct Rust avoids adding either runtime to the desktop
package. This is an architectural choice, not a measured SDK speed claim.
[Official SDK documentation](https://learn.chatgpt.com/docs/codex-sdk).

Keep the current exec transport available during evaluation and as the default.
The Settings transport choice is captured before request acceptance. Do not
silently switch an accepted request to another transport, provider, or model.

## Current source and reusable boundaries

| Current component | Migration responsibility |
| --- | --- |
| [`codex_runtime.rs`](../crates/core/src/providers/codex_runtime.rs), `start_with_choice` | The existing exec route remains the default and keeps its historical launch and receipt behavior. The app-server route is a separate Rust runtime. |
| [`codex_discovery.rs`](../crates/core/src/providers/codex_discovery.rs) | Performs bounded app-server initialization and paginated model discovery. That finite exchange remains separate from the persistent generation client. |
| [`codex_profile.rs`](../crates/core/src/providers/codex_profile.rs) and [`codex_app_server/launch.rs`](../crates/core/src/providers/codex_app_server/launch.rs) | Exec profiles retain their legacy bindings. App-server launch material uses its own app-owned home, restrictive catalog, ephemeral credential store, and validated per-request traits; exec-only flags are not copied blindly. |
| [`codex_app_server/runtime.rs`](../crates/core/src/providers/codex_app_server/runtime.rs) and [`windows_process.rs`](../crates/core/src/providers/cli/windows_process.rs) | Own the bounded JSONL reader, per-turn completion/cancellation, shared-process health, and eventual cleanup. |
| [`provider_runtime.rs`](../apps/desktop/src-tauri/src/provider_runtime.rs) | Owns optional server readiness, request admission, project/run routing, and normal close. The Settings transport preference never starts generation by itself. |
| [`packet.rs`](../crates/core/src/context/packet.rs), discussion/memory result storage, schema 38 | Persist versioned app-server bindings and separate delivery evidence while preserving old packets, hashes, receipts, and recovery behavior. |

Current author Codex bindings use `codex-stdin.author.v1`, with a 24 KiB exact
stdin-packet allowance and 64 KiB retained assistant-output allowance. These are
application byte limits, not a model context window or billing limit. Preserve
those content allowances in the first comparison; bound RPC envelopes and
protocol buffers separately. See [ADR 0011](ADR_0011_LIVE_CODEX.md) and
[ADR 0024](ADR_0024_DYNAMIC_CODEX_MODELS.md).

## Boundaries that stay in the story engine

- A saved application context snapshot determines permitted sources, narration
  policy, exact instructions, and editable scope. Provider conversation history
  does not replace it.
- Author-room secrets must not enter a later restricted writing request through
  a resumed conversation, cached tool state, or ambient instructions.
- Codex returns text or candidates. Neither a transport response nor a future
  proposal-submission tool adopts Workshop material or writes the manuscript.
- Existing source-head checks, stale refusal, atomic adoption/Apply, and local
  result recovery remain authoritative.
- New summary and Story Memory calls use GPT-6 Astra/low, per the author's
  8 September instruction, independently of the
  writing picker. Native maintenance retains its current requested priority
  tier; HTTP maintenance retains its existing no-tier contract.
- Codex stays unpinned. Record the checked executable identity and capabilities
  for a running server/request; qualify a newly installed executable before
  admitting work to a replacement server. Never mix identities in one receipt.

## Smallest useful first slice

The first implementation routes ordinary lookup-disabled discussion work,
including the existing selected-edit and continuation validators, through the
explicitly selected development transport. It uses the existing composer,
context inspector, model/effort/tier selection, immutable operation, streaming
display, and durable result paths. Fixed Story Memory maintenance can use the
same optional route with GPT-6 Astra and low reasoning. Opening a project or
picker still never generates.

Keep this choice in provider Settings/development configuration, out of the
model list and normal writing controls. The current UI exposes an explicit
Exec/app-server choice with Exec as the default. An accepted binding freezes the
transport even if Settings changes while the request runs.

If the prototype is qualified for only one exact model/trait configuration,
show other selected combinations as unavailable on app-server before request
acceptance. Preserve the picker choice. The author can explicitly select exec
or recheck the chosen configuration when the server is idle; do not silently
route unsupported choices through exec.

The current development route admits up to eight active app-server requests
through the existing admission limit. It still exposes rejection before a ninth
request rather than creating a hidden paid-work queue. Exec keeps its existing
behavior. Before app-server becomes the default, qualify independent concurrent
requests and per-project Stop, because global ordering and recovery behavior
still require evidence.

Lookup remains on exec because its bounded multi-turn allowance has not been
migrated. Native story tools, upstream conversation resume, context-cap
increases, retrieval changes, and additional adapters remain excluded. Workshop,
scoped proposals, and continuation are subsequent parity checks; fixed
maintenance is implemented only for the Astra/low contract described above.

## Runtime design

Use one application-owned server for a compatible checked account/configuration
generation, started lazily through the existing connection boundary. The native
runtime owns it, so tab navigation does not destroy active work. Use bounded
request channels and one protocol reader; do not introduce another scheduler or
story state machine. No remote listener is needed.

Correlate RPC responses by request ID and route generation events using server
generation, upstream thread/turn IDs, and the existing project/run owner. Local
discussion IDs are not upstream Codex thread IDs. Old-generation or terminal
events cannot attach to a newer request. Keep ignored notifications bounded;
fail required malformed messages and unsupported server requests safely.

The protocol provides JSONL stdio, response correlation, incremental assistant
text, and turn interruption. Dynamic tools are experimental. The documentation
also includes app-server/remote production-support warnings; using local stdio
does not itself qualify V3. Inspect schemas generated by the installed binary
before selecting required fields or claiming isolation behavior.
[Official app-server documentation](https://learn.chatgpt.com/docs/app-server).

### Isolation and configuration are a prerequisite

Before sending any story text, demonstrate fresh ephemeral-thread behavior and
equivalent exclusion of ambient rules, user configuration, memory, skills,
plugins, apps, filesystem/network tools, and unrelated history. Fresh threads
alone do not prove all of these properties. Do not silently accept persistent
upstream story logs if ephemeral behavior cannot be qualified. Keep author
credentials outside application logs and do not alter the author's Codex state
to make the prototype work.

**Installed-runtime finding (8 September):** app-server does not accept exec's
`--ignore-user-config` or `--ignore-rules` flags. Synthetic hostile-home checks
showed that `project_doc_max_bytes=0` does not suppress home `AGENTS.md`, and
`mcp_servers={}` does not remove lower-layer MCP entries. Consequently the new
transport uses a completely app-owned, empty `CODEX_HOME` and working directory.
Its explicit environment also retains Windows `SystemRoot` for platform
components; other author environment variables are not inherited. The first
live calls with only `CODEX_HOME` failed to connect, whereas adding `SystemRoot`
allowed the next call to stream a complete response.
It must reject a thread acknowledgment with nonempty `instructionSources`.

The local ChatGPT login is handed to that isolated server through the unstable
`account/login/start` external-token interface. A bounded read-only adapter reads
only the access token and account ID from the existing file store; refresh tokens
are not imported, rotated, or persisted. The new process uses ephemeral credential
storage. Credentials never enter project packets, application settings, or logs.
Keyring-only, API-key, malformed, and expired file-store credentials remain
unavailable through this route. This deliberately extends ADR 0011's exec-only
credential boundary for the new transport; historical exec behavior is unchanged.
The runtime identity retains only account/configuration/catalog hashes. Account
changes cannot redirect existing work, and authentication failure never authorizes
a replacement generation. The installed no-generation check now passes its
account handoff, restrictive catalog, fresh-thread, pre-turn cancellation, warm
reuse, and cleanup assertions on Codex 0.153.4. It does not perform a paid turn
and therefore does not qualify live generation, native UI behavior, or narrative
quality.

The app-server launch now writes one immutable restrictive multi-model catalog
into its app-owned temporary home. A shared server does not rewrite that file
between turns; each accepted request validates its exact model and traits
against the checked catalog. Do not add a process pool speculatively. A server
requiring configuration replacement must drain existing work before
replacement.

Reuse initially requires the same checked executable identity, transport/profile
version, security-configuration hash, and restrictive-catalog hash. Include the
selected model capability identity and traits when they are process-level
configuration. Only relax that last restriction after per-request overrides
are qualified. Reconfiguration must not mutate a server serving accepted work.

When an installed update is detected, let already-dispatched work finish on its
recorded server identity unless that server itself fails. Block new admission
to the old generation, drain it, and qualify a replacement. A healthy running
request is not poisoned merely because a newer Codex executable exists.

Explicitly test Luna → another model → Luna and Fast → Standard → Fast. Standard
must not inherit a priority default from a previous request or the server's
maintenance configuration. Freeze requested traits; keep effective traits
unknown unless the runtime actually supplies reliable evidence. Changing the
picker cannot redirect an accepted request.

An omitted tier currently permits the catalog/default behavior; it is not a
universal explicit Standard value. Either qualify explicit per-request values
or a clear-default operation, or keep mixed-tier reuse unavailable. Omission
alone cannot establish that an inherited default was cleared.

### Completion, Stop, and close

Keep provisional text distinct from the validated final result. Incremental
display must never make a partially streamed structured proposal adoptable.
Validate final item identity and its relationship to any persisted text prefix;
do not silently overwrite durable partial text after a mismatch.

| Situation | Required behavior |
| --- | --- |
| Successful terminal turn | Validate and persist its result; keep a healthy server available. |
| Stop before dispatch | Settle locally without submitting a turn. |
| Stop during generation | Interrupt the exact owned turn, wait for terminal settlement, and retain safe partial output. In the single-request slice, reuse the server only if its protocol state remains trustworthy; stage D must additionally keep other turns running. A cancellation acknowledgment alone is not terminal proof. |
| Stop races with completion | Preserve the existing author-Stop precedence and proposal suppression. |
| Lost start response after submission | Retain uncertainty and correlate subsequent events if possible. Never resend `turn/start` to discover whether the first one worked. |
| Server dies or framing becomes unusable | Stop admission and settle every affected request conservatively. A replacement server does not replay those requests. |
| One turn cannot be interrupted reliably | Escalate to owned server shutdown if needed; reflect the interruption/uncertainty in every affected request rather than reporting unrelated turns successful. |
| Result exists but local save fails | Retry only local persistence through existing recovery; do not generate again. |
| Normal app close | Block new admission, resolve active requests and retained results using the existing close policy, then terminate the owned server and verify cleanup. An idle server alone must not appear as active AI work. |

Release completed thread resources using qualified runtime behavior. Bound
retained threads and memory over many sequential requests; an unsubscribe
acknowledgment is not proof of immediate resource release. An idle recycle may
release resources after all work settles; it must not interrupt another project.

## Persistence and delivery contract

Introduce a new binding profile, provisionally `codex-app-server.author.v1`.
Add a separate maintenance profile when that route is migrated. The final names
and additive storage migration belong to implementation, not historical edits.

Persist the exact application packet plus deterministic transport parameters
before submission. Identify any transport envelope separately so volatile RPC
IDs do not change story-source identity. Record enough envelope/mapping evidence
to explain what was sent. Do not describe the application packet as the entirety
of Codex's internal system prompt or infer that the model read all available
story sources.

Persist the acknowledged upstream thread identity before submitting its turn,
and commit the owned dispatch claim, server generation, and RPC correlation ID
before writing `turn/start`. Its turn ID is not known in advance: persist it
when acknowledged or unambiguously observed in a correlated event. If that
observation is lost, record uncertainty rather than inventing an identity or
submitting again. Missing `thread/start` acknowledgment must not lead to a
generation on a guessed thread.

The first slice does not reconnect to ephemeral conversations after application
restart. Retained local results remain recoverable; unresolved upstream work is
interrupted/unknown and a new generation requires a new author request. Persisted
upstream IDs are evidence, not restart authority. Test worker loss with a known
turn, lost start acknowledgments, old-generation terminal events, and admission
on a replacement server without replay.

New receipts must distinguish local submission, acknowledged turn identity,
terminal outcome, request resource settlement, and connection/process cleanup
where relevant. `confirmed_stdin_bytes` currently measures exec input delivery;
neither a JSONL frame byte count nor a turn acknowledgment has that same
meaning. Do not stuff new semantics into this field or mark cleanup settled
merely because the shared process is still alive.

Reuse the existing operation/result tables where practical. Make the new
receipt representation explicit and extend validation and the reader floor
together. Old `codex-stdin.v1` and `codex-stdin.author.v1` bytes, hashes, receipts,
and backup behavior remain intact. A historical request can be inspected, but
cannot be upgraded and dispatched by reconciliation. Recovered or duplicated
projects cannot attach to an original project's upstream work.

## Separate follow-up tracks

**Transport parity:** the same fresh-thread route now covers ordinary Workshop
output, selected edits, chapter continuation, and fixed-model maintenance.
Reuse of their existing validators and adoption paths is implemented; native,
live-provider, cross-project, and failure-parity qualification remain follow-up
work.

**Context budgeting and retrieval:** evaluate larger useful context and search
results containing enough authorized excerpts to avoid unnecessary extra reads.
Measure these separately from transport reuse. Mandatory instructions and edit
scope cannot be silently truncated to improve timings.

**Native story tools:** later consider snapshot-bound `search_story`,
`read_story_source`, relationships, decisions, and recorded conflicts. These
names describe proposed application operations, not shipped APIs. A
`submit_workshop_proposal` operation would retain an unaccepted candidate through
Rust validation; it would not perform Apply. No arbitrary file paths, shell,
cross-project reads, or live-database writes become provider tools.

**Usage semantics:** existing C6 authorizes one invocation plus at most two
expansions, each with a durably frozen packet. Do not relabel this as three
app-server turns: a turn is not a guaranteed single model inference. Before
migrating lookup or enabling tools, qualify internal retries/continuations,
observable usage, and enforceable limits. Keep the existing route when exact
parity cannot be established; any different agent-work allowance needs its own
explicit contract and author-visible authorization. No compulsory planning call.

**Persistent author-room conversations:** defer until there are explicit rules
for history retention, source invalidation, disclosure, restore/copy behavior,
and unfinished upstream work. This is separate from keeping a runtime warm.

## Work order and evidence gates

| Stage | Deliverable and exit evidence |
| --- | --- |
| A — Protocol/isolation qualification | Codex 0.153.4 passes the installed no-generation settings, fresh-thread, account handoff, pre-turn cancellation, warm reuse, and cleanup check. Event routing is covered separately by synthetic protocol tests. |
| B — Owned Rust transport | Implemented bounded JSONL client, reader, lifecycle, per-turn cancellation, protocol failure and shutdown paths. Focused unit and synthetic checks pass; ordinary application behavior remains on exec by default. |
| C — One durable Discuss path | Implemented additive bindings/delivery receipts, explicit opt-in, frozen traits, incremental display, persistence/reopen and no-replay recovery. A live Luna/xhigh/priority Workshop request returned three validated candidates with exact durable receipts and confirmed shutdown. |
| D — Parity and concurrency | Synthetic tests cover interleaved requests, per-request Stop, lost acknowledgments, crashes, source fencing, and recovered-project isolation. Native Settings passes 4 checks and mock Workshop passes 31. Full native live-generation/recovery qualification remains open; lookup deliberately remains on exec. |
| E — Comparative trial | One synthetic Astra/low/priority packet passed Exec, first app-server, and warm app-server cases. Startup is measured separately; internal input-token counts differ. The representative workload matrix and native live lifecycle qualification remain open, with no general speed or narrative-quality claim. |
| F — Default decision | Make app-server the default only after the preceding gates demonstrate reliability and worthwhile UX/runtime benefit. Keep explicit fallback selection; never automatic resend. |

Use protocol fixtures and focused tests for development. Native application
automation belongs on an isolated test desktop or hosted Windows runner, never
the author's active desktop. A headless browser fixture does not establish
native process recovery or installed-package support. The installed no-generation
check passes on Codex 0.153.4, including Standard after priority and clean
shutdown. The isolated native Settings and Workshop checks and one successful
live Workshop result are recorded with the preceding failed trials in
[the qualification record](APP_SERVER_QUALIFICATION.md). These observations
do not establish installed-release support or a general performance improvement.

The separate local 7 September transport research note
(`docs/research/CODEX_TRANSPORT_LATENCY_2026_09_07.md`, outside this published checkpoint)
records earlier first text on the SDK/app-server route, with four server starts
at roughly 72–164 ms and fresh-thread creation at a 7.306 ms median. It also
explains the measurement boundary: exec first text was a completed message,
other routes exposed deltas, SDK startup was excluded from per-turn timings,
and proxy envelopes differed. These are existing development observations,
not new measurements or an isolated startup-speed result. The local research
directory is separate, currently untracked work; this ADR does not publish it.

For stage E, use identical saved application packets, output contracts, model,
effort, and requested tier, with controlled run order and reported sample counts.
Measure preparation, queueing, startup/handshake, submission, first visible text,
last text, terminal event, durable completion, usage, failures, and Stop
settlement. Include short discussion, Workshop alternatives, chapter continuation,
and the existing three-step lookup workflow as separate workloads; leave lookup
on exec until its allowance gate passes. Report cold and warm cases separately,
include failures, and do not call exec's completed-message time TTFT. A requested
Fast tier without an effective-tier echo remains unconfirmed.

Historical planning estimate: about one engineering day for a contained
prototype, roughly 3–5 days for an integrated fresh-thread discussion route, and
around a week including concurrency and native recovery qualification. These
estimates are retained for context; the implementation above supersedes the
prototype schedule. Native tools, larger context, and resumable author-room
history are separate work.
