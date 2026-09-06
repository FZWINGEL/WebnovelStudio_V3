# Codex qualification evidence

This document records historical versioned discovery and twenty bounded native CLI dispatches: three earlier direct CLI runs, one generated-profile run, two `CodexStream` runner qualifications, two native desktop edit requests, one native chapter-memory request, one native story-continuation request, one native structured-suggestion request, two native promise-context discussions, three invocations in one native story lookup, two current dynamic-author sends, one isolated diagnostic Mini invocation, and one explicit Mini follow-up. It does not establish a supported production provider. W8 and E3 remain open until provider, containment, failure, interruption, and release gates pass.

## Current provider policy

Codex is not version- or executable-hash-pinned. The adapter discovers the
installed CLI, checks its current launch and response surface, and records the
observed version/hash with each request. An update after connection check
requires a fresh check before dispatch; no older executable is silently chosen. The exact `0.153.3` identity below describes historical
qualification only. A 6 September 2026 C6 preflight found installed `0.153.4`
while the old profile required `0.153.3`; it made zero model calls and is
recorded as a compatibility finding below.

Background summary, chapter-memory, and other maintenance calls route to
GPT-5.6 Luna with xhigh reasoning. Author-facing writing and
revision uses the persistent V2-style model picker and selected traits. The
dynamic Codex discovery implementation is recorded in [ADR 0024](ADR_0024_DYNAMIC_CODEX_MODELS.md);
its bounded Luna/Mini qualification passes; broader native/provider/live and
release qualification remain pending. V2 CLI
adapter/catalog parity remains in progress. The configurable
OpenAI-compatible adapter now has native endpoint integration, twelve transport
tests, and native loopback-server qualification. See [ADR 0023](ADR_0023_OPENAI_COMPATIBLE.md)
and [current evidence](IMPLEMENTATION_STATUS.md#current-provider-checkpoint-codex-compatibility-and-http-development-surface).
It uses the same packet and explicit Apply ownership, with separate HTTP delivery
receipts, and never silently substitutes a provider or model.

## Current dynamic Codex implementation (bounded qualification)

An explicit Settings connection check now discovers author-facing Codex models
from the installed native CLI. The check records the observed CLI version and
executable SHA-256, runs bounded version, login, and strict compatibility
preflights, then performs an interactive app-server exchange of `initialize`,
`initialized`, and paginated `model/list` requests inside the owned Windows Job
Object. Discovery is bounded to 15 seconds, 1 MiB of output, 32 pages, and 256
models. A refresh replaces the app cache only after a complete sanitized
catalog is received; failure retains the previous cache.

The library schema is 4. The app-owned catalog stores model IDs, labels,
declared traits and defaults, observed CLI identity, and discovery time; it
stores no credentials or request text. Cached rows are display-only. The
current checked connection authorizes the exact selected model and concrete
traits. If a refresh removes a selected model or trait, the selection remains
visible and unavailable until the author explicitly repairs it; no automatic
substitution occurs.

Author-facing requests use `codex-stdin.author.v1`. The binding resolves the
selected model, reasoning, and service tier against the checked catalog and
records the observed runtime plus the sanitized descriptor fingerprint in
`runtime.catalogSha256`. The application caps remain 24 KiB input and 64 KiB
output; no provider token limit is inferred. The connection snapshot is cloned
into an in-flight request, so a refreshed descriptor cannot replay an old
binding, and saved results remain inspectable. A renderer request
acknowledgment does not trigger automatic generation replay.

Project schema 28 is the current reader floor; the schema-27 author-binding
boundary remains part of the compatibility contract and changes no project
tables. Legacy `codex-stdin.v1` bindings, including historical 0.153.3 packet
bytes and hashes, remain readable. Maintenance keeps
the fixed GPT-5.6-Luna/xhigh/priority profile and exposes separate
`memoryReady` state; an author-selected writing model does not redirect
maintenance work. Manual editing and the HTTP provider path are unchanged.

The implementation is complete as a development slice. The earlier provider
wrapper passed 589 active Rust tests (548 core and 41 desktop), one existing
ignored fixture, and 350 frontend tests in 27 files, with formatting, strict
Clippy, TypeScript, and the production build; this is dated provider evidence.
The current wrapper passes 593 active Rust tests (552 core and 41 desktop), one
existing ignored fixture, and 353 frontend tests in 27 files. The pre-parser-fix
dynamic development binary with SHA-256
`775962e975c7dc5b3f0171ba2d3724212b5921295eae897fd2052af92dcf7539` also
passes the native HTTP fixture with four local-mock POSTs, zero live calls,
and no page errors. The broad native diagnostic now passes 45/46 checks with
zero errors, omitting only the known local OS clipboard check; evidence is
`.local/native-other-results/report.json`, dated `2026-09-06T13:19:24.007Z`,
on WebView2 `152.0.4191.62`. The parser fix accepts a null `defaultServiceTier`,
handles valid non-text audio modalities while excluding audio-only rows, and
includes a sanitized 0.153.4 seven-model regression fixture. Focused catalog,
discovery, and integration checks plus strict workspace Clippy pass. The
current native build discovered seven models. The initial Luna/xhigh/priority
request completed; the first Mini/low request failed because an inherited
Luna-only `X-OpenAI-Internal-Codex-Responses-Lite` route was applied. Diagnostic
dispatch 19 confirmed that `model/list` does not declare that route; transport
now uses it only for Luna and standard Responses for other models, without
substitution. The explicit Mini/low/no-tier follow-up on binary SHA-256
`c9068efffda7a2ed08f81afd65f691fac411eabd43837f0a9257daac5830c533` completed
with usage 1477 input, 46 output, and 13 reasoning tokens, settled cleanup,
and no page errors. The bounded Luna/Mini qualification passes; the earlier
failure and successful manuscript remain retained, with no automatic replay.
The binary also passes the native HTTP fixture with live 0. A reopen-only check
on the prior binary passed two fresh native process launches: the cached
seven-model catalog stayed display-only/unready after restart, saved Mini/low/
null state and manuscript JSON/prose retained both exact identities, and
run/results counts stayed at two with zero new send, connection check,
generation, or page-error events. Source checkpoint
`7ce8b76f0d63e9c56d9bb8338ef8eb268079a465` has CI
[34033575745](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34033575745)
passing native strict 46 and the native HTTP fixture; the overall workflow
failed Ubuntu Clippy because a Windows-only credential helper was compiled
there. The helper is fixed in the current source, and the Windows contract jobs
were canceled after that failure. The bounded dynamic qualification is recorded
above; broader native/provider/live and release totals are not claimed here.

HTTP context lookup, wider V2 CLI adapters, broader live-provider support, and
release qualification remain pending. Cumulative dispatches now total twenty;
the bounded result does not qualify broader provider parity or HTTP-live
support.

### Dynamic selection dispatch ledger

All four dispatches below used synthetic text and the observed CLI 0.153.4.
The last two failures were not automatically retried. A fresh explicit request
qualified the corrected Mini profile. Missing usage on failed requests remains
unknown.

| Dispatch | Requested settings | Result |
| --- | --- | --- |
| 17 | Luna / xhigh / priority | Completed; 2,371 stdin bytes; reported 1,204 input and 250 output tokens, including 215 reasoning tokens |
| 18 | Mini / low / no service tier | Failed with the inherited Responses Lite transport; 3,386 stdin bytes, no assistant output, unknown usage, settled cleanup |
| 19 | Mini / low / no service tier; isolated diagnostic | Failed with `unsupported_value` for the Responses Lite model route; one tiny request, no author data |
| 20 | Mini / low / no service tier; corrected transport | Completed; 3,374 stdin bytes; reported 1,477 input and 46 output tokens, including 13 reasoning tokens |

Evidence is retained under `.local/live-dynamic-codex-qualification`,
`.local/wns-mini-diagnostic-_3klzina` (diagnostic at `2026-09-06T13:31:47Z`),
and `.local/live-mini-qualification` (finished `2026-09-06T13:36:46.055Z`).
The separate `.local/dynamic-codex-reopen-evidence` check finished at
`2026-09-06T13:34:36.663Z` with zero requests. The corrected transport's
native HTTP fixture finished at `2026-09-06T13:39:40.060Z`, using only four
loopback mock POSTs.

The full wrapper preceded the final transport and cursor-validation fixes.
After those narrow changes, the seven profile tests, six catalog unit tests,
four catalog integration tests, formatting, and strict workspace Clippy passed.
The successful native Mini run exercised the transport fix. The last cursor
change rejects malformed pagination without changing valid discovery results;
the next hosted run verifies the integrated final source.

## Current 0.153.4 lookup qualification (generations fourteen to sixteen)

This lookup ran on the accepted `8edd061` provider source, with a passing
540-Rust/325-frontend wrapper (`.local/provider-wrapper-accepted.log`). Its native
executable SHA-256 was
`50d3cf00bac1ef1957497cedab06e7a6ca900da158349cad3ee6058bd299e080`.
Later HTTP work and current checks are recorded in the
[provider checkpoint](IMPLEMENTATION_STATUS.md#current-provider-checkpoint-codex-compatibility-and-http-development-surface).
The 6 September live lookup used the current application profile,
GPT-5.6-Luna/xhigh/priority, and one immutable story basis. Invocation 1 searched
for an old compass promise; invocation 2 read its complete source; invocation 3
answered using the exact promise and the separate splint explanation. It did
not equate missing payoff evidence with proof that the promise was unfulfilled.

| Invocation | Confirmed stdin bytes | Input tokens | Output tokens | Reasoning output tokens |
| --- | ---: | ---: | ---: | ---: |
| Search request | 23582 | 5881 | 189 | 141 |
| Full-read request | 24521 | 6204 | 319 | 260 |
| Final answer | 23869 | 6436 | 1516 | 1413 |

All three results completed with settled local cleanup. Requested traits and
observed executable identity were retained, but effective upstream traits were
not echoed. The model could not name the chapter because the retrieved lookup
metadata lacked its display title. This is useful evidence of retrieval and
limited source use, not full task or narrative-quality acceptance.

The initial harness failed after these calls on its exact context-selector
label (`.local/live-lookup-qualification/qualification.json`). Same-data reopen
passed using the select ID and retained all three contexts plus unchanged prose
without another model request
(`.local/live-lookup-reopen-qualification/qualification.json`). The failed record
is preserved. At this historical checkpoint, cumulative live generations were
sixteen. Subsequent HTTP qualification
uses a local mock server; no real HTTP model service has been called.

## Historical installed identity and discovery (0.153.3)

The tested executable was the direct native desktop `codex.exe`, distinct from the PATH/npm shim. Its exact version output was `codex-cli 0.153.3`. The native `app-server` handshake also reported user-agent version `0.153.3`; a direct stdio launch used its own process and did not forward to an already-running desktop host.

The native `model/list` response exposed seven visible models:

`gpt-6-astra`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`, `gpt-5.5`, `gpt-5.4-mini`, and `gpt-5.3-codex-spark`.

The discovered Luna descriptor was `gpt-5.6-luna` / `GPT-5.6-Luna`, with supported efforts `low`, `medium`, `high`, `xhigh`, and `max`; default effort was `medium`. It advertised a `fast` speed tier and service tier `{ id: "priority", name: "Fast" }`. The native catalog did not expose V2's `flex` service-tier ID, so V3 must store the provider ID (`priority`) separately from the UI label (`Fast`).

The live `model/list` response exposed no context-window or maximum-output fields. A separate matching 0.153.3 local cache reported Luna `context_window: 272000`, `max_context_window: 872000`, and effective context `95%`; those values are cache-only evidence and are not an app-server capability contract.

## Primary-source capability and isolation findings

Native 0.153.3 `exec --help` confirmed `--sandbox read-only`, `--ephemeral`, `--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check`, `--cd`, `--json`, `--strict-config`, `-m`, and stdin prompt `-`. This version has no `--ask-for-approval` flag; the dispatches used `approval_policy="never"` as a config override. The official [noninteractive CLI documentation](https://learn.chatgpt.com/docs/non-interactive-mode) describes `--ignore-user-config` and `--ignore-rules`; neither is an all-surface tool or author-data isolation switch.

The restrictive profile supplied, through strict runtime `-c` overrides, a version-matched one-model catalog and these requested controls: `features.shell_tool=false`, `features.unified_exec=false`, `features.multi_agent=false`, `features.apps=false`, `features.plugins=false`, `project_doc_max_bytes=0`, `mcp_servers={}`, `agents.enabled=false`, empty skills/plugin/app instruction switches, and a `story-context` filesystem/network profile. The custom Luna descriptor set `shell_type=disabled`, `apply_patch_tool_type=null`, `experimental_supported_tools=[]`, `multi_agent_version=null`, and `supports_search_tool=false` while preserving identity, effort, and service-tier metadata. Removed compatibility aliases, including `apply_patch_freeform`, are not emitted by the current profile.

Exact 0.153.3 sources qualify only a narrower boundary:

- [`spec_plan.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/core/src/tools/spec_plan.rs) returns before ordinary shell registration when `shell_tool` is disabled or model metadata has `shell_type=Disabled`. `unified_exec=false` alone is insufficient; the feature was still reported as enabled by the installed feature listing even when disabled by overrides.
- [`features/src/lib.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/features/src/lib.rs) treats `apply_patch_freeform` as a removed compatibility key. Apply-patch availability is controlled separately by model metadata, and `multi_agent=false` is not stronger than a model-provided multi-agent version without the separate `[agents].enabled=false` control.
- [`config.schema.json`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/core/config.schema.json) and the exact-tag config/features source contain no documented `no_tools` or `protectedexperimental_no_tools` control. `web_search="disabled"` gates hosted search only; app, plugin, MCP, extension, dynamic, and other core sources are not a universal denylist.
- [`loader/mod.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/config/src/loader/mod.rs) applies `--ignore-user-config` to the user layer, including a user profile. [`overrides.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/config/src/overrides.rs) makes runtime `-c` overrides the highest local layer. The harness therefore supplied the profile through strict runtime overrides instead of relying on a profile file that the ignore flag would skip.
- [`models-manager/src/manager.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/models-manager/src/manager.rs) and [`model_info.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/models-manager/src/model_info.rs) show that `model_catalog_json` is a full metadata response and that requested model identity is retained while capability overrides are applied. This does not cause the stream to echo effective model traits.
- The permission profile in [`core/src/config/permissions.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/core/src/config/permissions.rs) and [`exec-server/src/process_sandbox.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/exec-server/src/process_sandbox.rs) is relevant to server-managed sandboxed child commands. The documented app-server `thread/shellCommand` path is unsandboxed full access; a V3 adapter must never expose it or forward unknown command, process, filesystem, MCP, or extension requests.

These controls reduce ordinary ambient surfaces but do not prove packet-only behavior, universal tool denial, or author-filesystem isolation. Effective model, effort, and service-tier identity remained unconfirmed in every JSONL stream. Upstream instructions, tool construction, retries, cancellation, and output budgets remain opaque.

## Pure launch profile prepared for the next adapter slice

The historical exported pure constructor in [`codex_profile.rs`](../crates/core/src/providers/codex_profile.rs) accepted version output and an app-owned catalog path, rejected every version other than `0.153.3`, and produced the then-current Windows stdin `exec` argument shape. That exact gate is historical evidence and must be replaced by compatibility discovery before the provider path is treated as current. The constructor did not locate or launch Codex, read auth/config state, write the catalog, or make a provider request.

Its requested Luna catalog keeps the discovered identity, `low`/`medium`/`high`/`xhigh`/`max` effort set, and `priority`/`Fast` service label while setting `tool_mode="direct"`, `shell_type="disabled"`, `apply_patch_tool_type=null`, `experimental_supported_tools=[]`, `multi_agent_version=null`, and `supports_search_tool=false`. Context limits remain absent because the live response did not expose them. Runtime overrides include `approval_policy="never"`, `web_search="disabled"`, empty MCP/plugins, disabled app/skill/instruction surfaces, `[agents].enabled=false`, disabled `tools.experimental_request_user_input` and `tools.update_plan`, hidden tool metadata, the `story-context` filesystem/network profile, and explicit 0.153.3 gates for shell, unified exec, request permissions, view image, sleep, deferred execution, token budget, current-time reminders, multi-agent, apps, plugins, MCP apps, skills, image/web/browser/computer/code-mode, goals, guardian, hooks, remote plugins, in-app surfaces, workspace dependencies, shell snapshots, capability discovery, retries, and elicitation. Removed/deprecated compatibility aliases are not emitted. The profile preserves `default_permissions="story-context"` through runtime overrides instead of adding the mutually exclusive `--sandbox` mode, and ends with `-` so the future adapter's packet remains stdin-only.

This is requested launch material and catalog metadata; it is not an observation that every registration path is absent. Exact source still has no universal no-tools or app-server allowlist control, and dynamic/extension contributors must be kept out by the future adapter and rejected if encountered. The direct profile run and the two `CodexStream` runs below used the generated arguments and catalog after the strict-config preflight passed.

## Dispatch ledger

The rows below are generation dispatches. The strict-config and missing-schema checks inside the isolation-profile runs were local preflights and did not send an LLM request. The current profile harness performed one dispatch; the runner harness performed two sequential dispatches with no retry; native desktop edit qualification performed two additional dispatches. The chapter-memory qualification below made one further dispatch, story-continuation qualification made one further dispatch, structured-suggestion qualification made one further dispatch, and promise-context qualification made two fresh discussion dispatches; their same-data verifications sent none.

| Dispatch | Configuration and auth boundary | Result | Evidence |
| --- | --- | --- | --- |
| `2026-09-05 19:46:42`, run `run-20260905-194642-359364d1` | Native 0.153.3; empty V3-owned working directory; `--sandbox read-only`; ignore user config/rules; model `gpt-5.6-luna`, effort `max`, service tier `priority`; restricted feature overrides | Success, exit `0`, about 11.3 seconds; one assistant message; no tool event; usage input `6999`, output `219`, reasoning output `141`; no effective identity echo | [initial run evidence](../.local/codex-live-qualification/run-20260905-194642-359364d1/) |
| `2026-09-05 20:30:52`, run `run-20260905-203052-433` | Dedicated empty `CODEX_HOME`; same native binary and restrictive runtime overrides; no `--sandbox` flag | Failure, exit `1`, `17073 ms`; 401 Unauthorized after repeated upstream reconnect attempts because the dedicated home had no inherited bearer credentials; no usage reported, no prose, no last message, no tool event | [isolated-home evidence](../.local/codex-isolation-profile/run-20260905-203052-433/) |
| `2026-09-05 20:35:59`, run `run-20260905-203559-601` | Child `CODEX_HOME` assignment removed; native default managed login reused for host auth; empty app-owned cwd; same ignore flags and strict runtime overrides; no `--sandbox` flag | Success, exit `0`, `6386 ms`; one 21-word assistant message and last-message file; no tool event; usage input `7897`, output `186`, reasoning output `155`; no effective identity echo | [normal-home evidence](../.local/codex-isolation-profile/run-20260905-203559-601/) |
| `2026-09-05 22:06:33`, current profile run | Rust `CodexLaunchProfile` materialized the one-model catalog and 170 stdin `exec` arguments; exact 0.153.3 strict-config preflight passed; empty app-owned cwd; ignore user config/rules; model `gpt-5.6-luna`, effort `max`, service tier `priority`; no CLI `--sandbox` because named `story-context` permissions are used | Success, exit `0`, `5382 ms`; one assistant message; no tool event; usage input `414`, output `172`, reasoning output `129`; startup JSONL contained one under-development warning for `skip_host_skill_discovery`; no effective identity echo; no retry | [current profile evidence](../.local/codex-runtime-qualification/) |
| `2026-09-05 22:10:33`, runner `complete` case | `CodexStream` over the existing Windows Job-contained process primitive; exact generated profile; inherited managed login; empty app-owned cwd; model `gpt-5.6-luna`, effort `max`, service tier `priority`; packet sent through stdin | Completed; cleanup settled; confirmed stdin `135/135` bytes; 141 assistant-text characters; no tool event; usage input `438`, output `137`, reasoning output `99`; warning count `1`; no retry | [runner evidence](../.local/codex-runner-qualification/run-1788646233/qualification.json) |
| `2026-09-05 22:10:33`, runner `stop` case | Same `CodexStream` and profile; a fresh 400-word synthetic packet; stop requested after eight seconds without a delta because the stream had not produced a terminal event | Stopped; cleanup settled; confirmed stdin `104/104` bytes; no assistant delta or usage; stop trigger `timer_without_delta`; no retry | [runner evidence](../.local/codex-runner-qualification/run-1788646233/qualification.json) |
| `2026-09-05 22:25:58`, native desktop integration | Built desktop executable; fresh synthetic English project and chapter; explicit connection check; `gpt-5.6-luna`, effort `max`, service tier `priority`; selected-passage Suggest edits request; one dispatch, no retry | Provider receipt completed; cleanup settled; confirmed stdin `2336` bytes; usage input `1204`, output `185`, reasoning output `161`; effective identity absent; response was ordinary prose and produced `0` retained proposals, so Apply/reload retention was not attempted | [native integration evidence](../.local/live-native-qualification/qualification.json) |
| `2026-09-05 22:38:51`, native desktop integration | Final qualification executable (`26,305,024` bytes, modified `2026-09-05T22:37:06.694Z`); fresh synthetic English project and chapter; explicit connection check; `gpt-5.6-luna`, effort `max`, service tier `priority`; selected-passage Suggest edits request; one dispatch, no retry | Provider receipt completed; cleanup settled; confirmed stdin `3360` bytes; usage input `1375`, output `188`, reasoning output `144`; effective identity absent; one retained structured proposal was explicitly applied and the protected ending persisted. Same-data reopen, requested trait details, and `Used` context state passed in the read-only continuation; the initial post-Apply harness stopped on an ambiguous context-inspector selector and made no second dispatch | [initial qualification evidence](../.local/live-native-qualification-v2/qualification.json), [same-data continuation](../.local/live-native-qualification-v2/continuation.json) |

The first, third, current profile, runner, and native desktop requests used the same requested Luna/Max/priority identity, but the upstream JSONL reported no effective model, effort, or service tier. Those values are exact application request values, not independently verified provider traits. The direct profile's materialized catalog and argument hashes, prompt hash, process identity, exit, event observation, and cleanup status are recorded in `generation-metadata.json`; its raw JSONL is retained in `stdout.jsonl`. The runner and native desktop cases retain only typed, sanitized results in `qualification.json`.

The seventh request exposed a compiler gap for live structured edits: its frozen packet had no response-format instruction, so its ordinary prose response correctly yielded zero proposals under the strict JSON retention contract. The current build includes the fixed compiler-owned `proposal-output.v1` revise instruction, leaving the final author request unchanged and requiring the exact JSON keys, one to three candidates, single-line selected-passage replacements, and bounded explanations. The eighth request exercised that contract and retained one structured proposal, which the native flow applied and reloaded successfully. This evidence does not relax the parser or infer candidates from prose.

### Ninth dispatch: native chapter memory

At `2026-09-06T01:02:03.634Z`, the native app used a fresh synthetic English chapter and the explicitly checked GPT-5.6-Luna/Max/priority profile for one **Refresh story memory** request. Executable SHA `abde8c8a1a9be9d90a711398fc146fdbdbae7c24c06d038db8ad140653f4d9b5`, 28,552,704 bytes, built `2026-09-06T01:01:05.5466457Z`. The immutable packet retained the full chapter with no omissions. The result completed with settled cleanup, `3019` confirmed stdin bytes, `1257` reported input tokens, `1931` output tokens, and `1552` reasoning-output tokens. Effective provider identity remains unknown.

The strict `navigation-digest.v1` response retained three items, each with exact validated source quotations, and installed one generated view. It distinguished the key handover, the unfulfilled-on-page promise, and Mei's lack of knowledge about the changed lock. This small inspection does not establish semantic reliability, story-wide coverage, or narrative quality.

The first harness stopped after generation because its Windows temporary-path comparison rejected the saved path. The [initial report](../.local/live-memory-qualification/qualification.json) remains a failed harness record. A [same-data continuation](../.local/live-memory-qualification/continuation.json), completed `2026-09-06T01:03:42.216Z`, validated the result, exact source inspection, unchanged manuscript, and retained memory after reopening. It made zero model requests; the flow's total is one. No renderer errors occurred. Both owned app PIDs were absent after cleanup; the harness did not capture a normal-close exit code. This is a narrow live-memory development qualification, separate from installed-release and complete W8/E3 gates.

### Tenth dispatch: native story continuation

The live Working-draft continuation request ran from `2026-09-06T03:19:18.761Z` to `2026-09-06T03:19:35.554Z` in the rebuilt native application using executable SHA `cbb036fc97da1f9f8b9e358c314d9eaf5770db909ade8e92910705f2009d2787`. It requested GPT-5.6-Luna with Max reasoning and the `priority` service tier; the effective provider identity was absent. The provider completed with settled local cleanup and returned two valid English paragraphs. The native flow previewed and applied them while preserving the original chapter. Confirmed stdin was `3424` bytes; reported usage was `1410` input tokens and `267` output tokens, including `159` reasoning tokens; errors were zero.

The [initial qualification evidence](../.local/live-continuation-qualification/qualification.json) records one generation. The [reconciliation evidence](../.local/live-continuation-qualification/reconciliation.json) verifies one proposal, one prepared version, one decision, and one provider receipt. This same-data verification, completed `2026-09-06T03:22:39.577Z`, reopened the exact applied body and decision without another model request. The initial post-Apply harness failed on a Windows extended-path comparison; that failed record remains preserved and was not treated as a second live attempt. The reopened screenshot was inspected. This is one bounded live continuation result. [CI 34008911179](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34008911179) subsequently passed Windows/Ubuntu contracts and all 36 native checks on checkpoint `430831fc133dbe37be47fa477d7c3ea3b312e505`; broader provider failure/cleanup coverage and release qualification remain open.

### Eleventh dispatch: native structured suggestion

The bounded structured-suggestion request ran from `2026-09-06T06:42:21.292Z`
to `2026-09-06T06:42:38.878Z` in the same qualification executable
(`ca114dc73db5470332fd7b797c56380eb5244e303fa3595ca78c782eac685b15`,
30,628,864 bytes, built `2026-09-06T06:40:00.255Z`). It requested
`gpt-5.6-luna` with Max reasoning and the `priority`/Fast service tier; the
effective provider identity was not reported. The provider completed with
settled cleanup and zero errors, returning one candidate titled “The promised
key” with two English paragraphs and an italic “silver key” phrase. Confirmed
stdin was 3,850 bytes; reported usage was 1,500 input tokens and 506 output
tokens, including 345 reasoning tokens. The native flow edited and applied the
rich preview in the same editor, then reopened the exact body and decision;
the retained records were one proposal, one prepared version, one decision,
and one provider result. Evidence is
`.local/live-structured-qualification/qualification.json` and
`.local/structured-live.log`.

The reopened screenshot was inspected and the literal prompt constraints and
event facts were preserved. This is a bounded integration result, not a
narrative-quality evaluation or general provider qualification. The current
structured suggestion contract is documented in [ADR 0020](ADR_0020_STRUCTURED_SUGGESTIONS.md).
## Native promise-context discussion (twelfth generation)

On 6 September 2026, one explicit native Author Room discussion used requested
GPT-5.6-Luna, Max reasoning, and priority service tier after the native Settings
connection check. The synthetic project had an earlier exact reviewed promise
and a later working chapter. Recording the promise, preparing review, opening
history, and reopening the project made no model call.

The exact packet delivered the reviewed setup observation and its original
passage. The completed response quoted the promise and correctly stated that
missing payoff evidence did not prove the key was never returned. It could not
name the earlier chapter because the packet lacked chapter titles. This is an
observed input gap, not a successful chapter-citation result. The subsequent
Author Room source-label correction is qualified separately.

The run completed between `2026-09-06T07:41:57.372Z` and
`2026-09-06T07:42:17.237Z`. Provider-reported usage was 1,887 input tokens and
580 output tokens, including 501 reasoning tokens. The provider reported
settled cleanup and no error; effective model settings remained unknown.
Exactly one discussion run and provider result were retained, with zero
proposals or decisions. Prose remained unchanged and the discussion survived
reopening. This checks delivery and one evidence answer, not general retrieval,
understanding, narrative quality, or provider reliability.

Evidence is retained in
`.local/live-promise-initial-qualification/qualification.json` and its native
screenshots. The executable SHA-256 was
`4ca9c84ef040b328e68f6d2866e89d66e2eca710c7612bbfa566bca2c53365bc`.

## Named promise evidence (thirteenth generation)

One fresh synthetic native discussion verified the packet-v2 correction on
6 September 2026, from `2026-09-06T07:58:34.038Z` to
`2026-09-06T07:58:51.353Z`. The requested profile remained
GPT-5.6-Luna/Max/priority. This was a new explicit generation in a separate
project, not a replay of the earlier saved response.

The exact 4,333-byte delivered input contained frozen Author Room chapter
names, the reviewed promise, and its exact original evidence. The response
named **The key and the promise**, quoted Ren's promise, and distinguished
missing payoff evidence from proof of non-occurrence. One completed discussion
run and one provider result were retained, with no proposal or decision and
unchanged prose. Reopening retained the discussion without another call.

Provider-reported usage was 1,917 input tokens and 426 output tokens, including
344 reasoning tokens. Cleanup was settled; the result had no error. Effective
model settings remained unknown. The local input-byte allowance is separate
from these reported token counts. This qualifies one named evidence answer,
not general narrative understanding or writing quality.

Evidence is `.local/live-promise-named-qualification/qualification.json`, its
native screenshots, and `.local/promises-live-named.log`. Executable SHA-256 was
`b615a6c888cd5e48085967f44db582604c07807522920615c7722870f7bc48e1`, built
`2026-09-06T07:57:34.418Z`. The packet-v2 labels are Author Room-only;
restricted title exclusion has deterministic and native development evidence,
not a live restricted-generation claim from these two discussions.

The earlier C6 live-lookup harness was run on 6 September 2026 and stopped
during compatibility preflight because the installed CLI reported `0.153.4`
while the old profile required `0.153.3`. It made **zero model calls**; the
record is `.local/live-lookup-preflight-01533/qualification.json`. This is a
historical compatibility finding, not a failed generation; current cumulative
native dispatches remain twenty. The current source-title projection is an
application packet change under native/full-wrapper qualification. No C6 live
result, provider-native function-calling claim, model-specific token-budget
claim, or narrative-quality conclusion is recorded here. The current
source-title projection change made no new LLM calls.

## Current C6 frozen source-title projection

The source-title projection is implemented for new child lookup packets only.
It carries exact frozen handles, `SourceRef` values, and display names for
returned search/read sources; absent initial or historical source sets remain
absent. The strict native set is now 47 checks after frozen-rename/reopen
coverage. The current local diagnostic passes 46/47 with zero errors, omitting
only the known local OS clipboard check, on WebView2 `152.0.4191.62` using
executable SHA-256
`a66e07b155d1aedc24588e6d7b388f02bffb5e790c7d9991c5f3640206788cfe`.
Serialized input and receipts retain the frozen title, and UI rename/reopen
leaves packet JSON unchanged. The current wrapper passes 593 active Rust tests
(552 core and 41 desktop), one existing ignored fixture, and 353 frontend tests
in 27 files, with formatting, strict Clippy, TypeScript, and the production
build. The strengthened migration checks also pass the focused dynamic-author
binding case and 13-case full-context migration set. No broader provider or
live-lookup qualification is claimed.

## Authentication, sources, and cleanup limits

The successful normal-home run used the native executable's existing managed login without reading, copying, or extracting auth/config/credential values. `--ignore-user-config` still excluded the user config layer and `--ignore-rules` excluded user/project execpolicy rules. The dedicated-home 401 demonstrates that an empty isolated auth home cannot be assumed to carry the managed login. Host authentication therefore remains intentionally unisolated; the app-owned working directory and generated catalog/profile/run artifacts were isolated from manuscript sources.

No tool event was observed in any generation stream recorded here. This is observation evidence only and does not qualify universal no-tools or filesystem isolation. Both runner cases and all native desktop cases reported settled local cleanup; the explicit Stop case confirms the runner's local stopped/cleanup result but does not establish upstream cancellation or billing cessation. The normal-home run also left a separately pre-existing native Codex process, so no global process-absence claim is made. The direct profile harness did not exercise the Job foundation (`descendant_job_containment_tested=false`); the runner harness and native desktop path did.

The Windows process foundation documents the bounded child-tree, stdin/stdout, Job Object, partial-output, cancellation, and Drop contracts in [Windows child-process contract](WINDOWS_PROCESS_CONTRACT.md). Runner qualification exercises that foundation through `CodexStream`; the subsequent native cases exercise its bounded discussion integration. Full provider qualification remains open.

## Subsequent unexecuted correction

After the normal-home run, the older ignored harness was corrected to use the supported top-level `web_search = "disabled"` setting and to remove the deprecated `[features].web_search` alias. The normal run itself emitted the upstream deprecation warning for that old alias; that correction was not reexecuted in the old harness. The current profile row above was built with the supported top-level setting and is recorded separately.

## Remaining W8/E3 gates

These dispatches did not qualify the full refusal/truncation and broken/partial-stream matrix, provider-side cancellation or billing cessation, all recovered terminal outcomes, credential-entry/storage leakage, model token budgeting, general structured-output reliability, effective model-trait reporting, or provider-managed retry accounting. The eighth request established one native structured edit and same-data reopen, and the tenth established one native continuation and same-data reopen; neither closes those broader gates. The one synthetic Stop case only establishes the local `CodexStream` status and cleanup signal. They also did not prove that disabled feature settings remove every tool source. V3 must preserve upstream failures and partial output, fail closed on unexpected requests, and avoid claims of exactly-once external execution or billing.

The bounded Windows development path is available only after an explicit
installed-CLI compatibility check. Historical dispatches used the
Luna/Max/priority binding; background summary and memory work now target
Luna/xhigh by policy, while author-facing requests are intended to follow the
model picker. Other choices remain subject to adapter qualification, and full
provider support is still unqualified. Manual offline writing remains
independent of this evidence. See [W8 in implementation status](IMPLEMENTATION_STATUS.md#W8--one-qualified-live-provider), [E3 in the first-slice plan](V3_FIRST_SLICE_PLAN.md#5-experiments-that-can-change-the-architecture), and the provider/lifecycle contract in [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md#11-providers-jobs-and-interruption).

Official references used during qualification: [noninteractive mode](https://learn.chatgpt.com/docs/non-interactive-mode), [sandboxing](https://learn.chatgpt.com/docs/sandboxing), and the [Codex app-server lifecycle](https://learn.chatgpt.com/docs/app-server). Additional exact-tag source links appear above; ignored local source notes are retained in `../.local/codex-contract-research.md`.
