# Codex qualification evidence

This document records versioned discovery and exactly three bounded native CLI dispatches. It does not establish a supported production provider. W8 and E3 remain open until provider, containment, failure, interruption, and release gates pass.

## Exact installed identity and discovery

The tested executable was the direct native desktop `codex.exe`, distinct from the PATH/npm shim. Its exact version output was `codex-cli 0.153.3`. The native `app-server` handshake also reported user-agent version `0.153.3`; a direct stdio launch used its own process and did not forward to an already-running desktop host.

The native `model/list` response exposed seven visible models:

`gpt-6-astra`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`, `gpt-5.5`, `gpt-5.4-mini`, and `gpt-5.3-codex-spark`.

The discovered Luna descriptor was `gpt-5.6-luna` / `GPT-5.6-Luna`, with supported efforts `low`, `medium`, `high`, `xhigh`, and `max`; default effort was `medium`. It advertised a `fast` speed tier and service tier `{ id: "priority", name: "Fast" }`. The native catalog did not expose V2's `flex` service-tier ID, so V3 must store the provider ID (`priority`) separately from the UI label (`Fast`).

The live `model/list` response exposed no context-window or maximum-output fields. A separate matching 0.153.3 local cache reported Luna `context_window: 272000`, `max_context_window: 872000`, and effective context `95%`; those values are cache-only evidence and are not an app-server capability contract.

## Primary-source capability and isolation findings

Native 0.153.3 `exec --help` confirmed `--sandbox read-only`, `--ephemeral`, `--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check`, `--cd`, `--json`, `--strict-config`, `-m`, and stdin prompt `-`. This version has no `--ask-for-approval` flag; the dispatches used `approval_policy="never"` as a config override. The official [noninteractive CLI documentation](https://learn.chatgpt.com/docs/non-interactive-mode) describes `--ignore-user-config` and `--ignore-rules`; neither is an all-surface tool or author-data isolation switch.

The restrictive profile supplied, through strict runtime `-c` overrides, a version-matched one-model catalog and these requested controls: `features.shell_tool=false`, `features.unified_exec=false`, `features.apply_patch_freeform=false`, `features.multi_agent=false`, `features.apps=false`, `features.plugins=false`, `project_doc_max_bytes=0`, `mcp_servers={}`, `agents.enabled=false`, empty skills/plugin/app instruction switches, and a `story-context` filesystem/network profile. The custom Luna descriptor set `shell_type=disabled`, `apply_patch_tool_type=null`, `experimental_supported_tools=[]`, `multi_agent_version=null`, and `supports_search_tool=false` while preserving identity, effort, and service-tier metadata.

Exact 0.153.3 sources qualify only a narrower boundary:

- [`spec_plan.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/core/src/tools/spec_plan.rs) returns before ordinary shell registration when `shell_tool` is disabled or model metadata has `shell_type=Disabled`. `unified_exec=false` alone is insufficient; the feature was still reported as enabled by the installed feature listing even when disabled by overrides.
- [`features/src/lib.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/features/src/lib.rs) treats `apply_patch_freeform` as a removed compatibility key. Apply-patch availability is controlled separately by model metadata, and `multi_agent=false` is not stronger than a model-provided multi-agent version without the separate `[agents].enabled=false` control.
- [`config.schema.json`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/core/config.schema.json) and the exact-tag config/features source contain no documented `no_tools` or `protectedexperimental_no_tools` control. `web_search="disabled"` gates hosted search only; app, plugin, MCP, extension, dynamic, and other core sources are not a universal denylist.
- [`loader/mod.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/config/src/loader/mod.rs) applies `--ignore-user-config` to the user layer, including a user profile. [`overrides.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/config/src/overrides.rs) makes runtime `-c` overrides the highest local layer. The harness therefore supplied the profile through strict runtime overrides instead of relying on a profile file that the ignore flag would skip.
- [`models-manager/src/manager.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/models-manager/src/manager.rs) and [`model_info.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/models-manager/src/model_info.rs) show that `model_catalog_json` is a full metadata response and that requested model identity is retained while capability overrides are applied. This does not cause the stream to echo effective model traits.
- The permission profile in [`core/src/config/permissions.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/core/src/config/permissions.rs) and [`exec-server/src/process_sandbox.rs`](https://github.com/openai/codex/blob/rust-v0.153.3/codex-rs/exec-server/src/process_sandbox.rs) is relevant to server-managed sandboxed child commands. The documented app-server `thread/shellCommand` path is unsandboxed full access; a V3 adapter must never expose it or forward unknown command, process, filesystem, MCP, or extension requests.

These controls reduce ordinary ambient surfaces but do not prove packet-only behavior, universal tool denial, or author-filesystem isolation. Effective model, effort, and service-tier identity remained unconfirmed in every JSONL stream. Upstream instructions, tool construction, retries, cancellation, and output budgets remain opaque.

## Three-dispatch ledger

The three rows below are generation dispatches. The strict-config and missing-schema checks inside the two isolation-profile runs were local preflights and did not send an LLM request.

| Dispatch | Configuration and auth boundary | Result | Evidence |
| --- | --- | --- | --- |
| `2026-09-05 19:46:42`, run `run-20260905-194642-359364d1` | Native 0.153.3; empty V3-owned working directory; `--sandbox read-only`; ignore user config/rules; model `gpt-5.6-luna`, effort `max`, service tier `priority`; restricted feature overrides | Success, exit `0`, about 11.3 seconds; one assistant message; no tool event; usage input `6999`, output `219`, reasoning output `141`; no effective identity echo | [initial run evidence](../.local/codex-live-qualification/run-20260905-194642-359364d1/) |
| `2026-09-05 20:30:52`, run `run-20260905-203052-433` | Dedicated empty `CODEX_HOME`; same native binary and restrictive runtime overrides; no `--sandbox` flag | Failure, exit `1`, `17073 ms`; 401 Unauthorized after repeated upstream reconnect attempts because the dedicated home had no inherited bearer credentials; no usage reported, no prose, no last message, no tool event | [isolated-home evidence](../.local/codex-isolation-profile/run-20260905-203052-433/) |
| `2026-09-05 20:35:59`, run `run-20260905-203559-601` | Child `CODEX_HOME` assignment removed; native default managed login reused for host auth; empty app-owned cwd; same ignore flags and strict runtime overrides; no `--sandbox` flag | Success, exit `0`, `6386 ms`; one 21-word assistant message and last-message file; no tool event; usage input `7897`, output `186`, reasoning output `155`; no effective identity echo | [normal-home evidence](../.local/codex-isolation-profile/run-20260905-203559-601/) |

The first and third requests used the same requested Luna/Max/priority identity, but the upstream JSONL reported no effective model, effort, or service tier. Those values are exact application request values, not independently verified provider traits. The custom catalog SHA-256 was `9A39A0350F3E25ADB7A5B4D33A398BEF92CC28C1CC9F74D1FA90017BED5CA1A8`; the normal-home run records its request/profile hashes and event counts in `requested.json`, `metadata.json`, and `events-summary.json`.

## Authentication, sources, and cleanup limits

The successful normal-home run used the native executable's existing managed login without reading, copying, or extracting auth/config/credential values. `--ignore-user-config` still excluded the user config layer and `--ignore-rules` excluded user/project execpolicy rules. The dedicated-home 401 demonstrates that an empty isolated auth home cannot be assumed to carry the managed login. Host authentication therefore remains intentionally unisolated; the app-owned working directory and generated catalog/profile/run artifacts were isolated from manuscript sources.

No tool event was observed in any of the three generation streams. This is observation evidence only and does not qualify universal no-tools or filesystem isolation. The first run's single-process cleanup and the later harness's owned-process exit do not qualify descendant containment, Windows Job Object behavior, or cleanup after a tool or cancellation path. The normal-home run also left a separately pre-existing native Codex process, so no global process-absence claim is made.

## Subsequent unexecuted correction

After the normal-home run, the ignored harness was corrected to use the supported top-level `web_search = "disabled"` setting and to remove the deprecated `[features].web_search` alias. The normal run itself emitted the upstream deprecation warning for that old alias; the corrected harness was not reexecuted, so the correction has no live qualification result. It does not change the three-dispatch ledger above.

## Remaining W8/E3 gates

These dispatches did not qualify refusal or truncation, broken or partial streams, Stop and cancellation linearization, descendant cleanup, recovered terminal history, credential-entry/storage leakage, context-budget enforcement, structured output, effective model-trait reporting, or provider-managed retry accounting. They also did not prove that disabled feature settings remove every tool source. V3 must preserve upstream failures and partial output, fail closed on unexpected requests, and avoid claims of exactly-once external execution or billing.

The first supported provider remains unavailable. Production discussions remain mock-only and provider support is still unqualified. Manual offline writing remains independent of this evidence. See [W8 in implementation status](IMPLEMENTATION_STATUS.md#W8--one-qualified-live-provider), [E3 in the first-slice plan](V3_FIRST_SLICE_PLAN.md#5-experiments-that-can-change-the-architecture), and the provider/lifecycle contract in [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md#11-providers-jobs-and-interruption). No further live requests were made for this update.

Official references used during qualification: [noninteractive mode](https://learn.chatgpt.com/docs/non-interactive-mode), [sandboxing](https://learn.chatgpt.com/docs/sandboxing), and the [Codex app-server lifecycle](https://learn.chatgpt.com/docs/app-server). Additional exact-tag source links appear above; ignored local source notes are retained in `../.local/codex-contract-research.md`.
