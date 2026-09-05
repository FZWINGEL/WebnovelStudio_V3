# Codex qualification evidence

This document records bounded discovery and one synthetic native CLI request. It does not establish a supported production provider. V3 W8 and E3 remain open until the full provider and containment gates pass.

## Exact installed identity and discovery

The qualified executable was the direct native desktop `codex.exe`, distinct from the PATH/npm shim. Its exact version output was `codex-cli 0.153.3`. The native `app-server` handshake returned user-agent version `0.153.3`; a direct stdio launch used its own process and did not forward to the already-running desktop host.

The native `model/list` response exposed seven visible models:

`gpt-6-astra`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`, `gpt-5.5`, `gpt-5.4-mini`, and `gpt-5.3-codex-spark`.

The discovered Luna descriptor was `gpt-5.6-luna` / `GPT-5.6-Luna`, with supported efforts `low`, `medium`, `high`, `xhigh`, and `max`; default effort was `medium`. It advertised an additional speed tier `fast` and a service tier `{ id: "priority", name: "Fast" }`. The current native catalog did not expose V2's `flex` service-tier ID; V3 must record the provider ID (`priority`) separately from its UI label (`Fast`).

The live `model/list` response exposed no context-window or maximum-output fields. A separate versioned local model cache reported Luna `context_window: 272000`, `max_context_window: 872000`, and effective context `95%`; these are cache-only evidence from the matching 0.153.3 cache, not the app-server contract. Do not promote them to a V3 capability limit without a qualified source.

## Verified noninteractive controls

Native 0.153.3 `exec --help` confirmed these controls:

- `--sandbox read-only`;
- `--ephemeral`;
- `--ignore-user-config` (config file omitted; managed authentication remains an upstream concern);
- `--ignore-rules` (user/project execpolicy files omitted);
- `--skip-git-repo-check`, `--cd`, `--json`, `--strict-config`, `-m`, and stdin prompt `-`.

This version has no `--ask-for-approval` flag. The qualification used `approval_policy="never"` as a config override. The tested override set also requested `features.shell_tool=false`, `features.unified_exec=false`, `features.apply_patch_freeform=false`, `features.multi_agent=false`, `features.apps=false`, `features.plugins=false`, `web_search="disabled"`, `project_doc_max_bytes=0`, and `mcp_servers={}`.

The 0.153.3 `features list` check showed `shell_tool=false`, `apps=false`, `plugins=false`, and `multi_agent=false`, but `unified_exec=true` even when disabled with both `-c` and `--disable`. Treat the unified-exec override as ineffective in this installation. No single documented no-tools control was found.

These settings reduce ordinary ambient surfaces but do not prove universal packet-only or author-filesystem isolation. The app-server/CLI may add internal instructions, extensions, or other tool sources; the final upstream payload and any CLI-managed retries remain opaque. V3 must fail closed on unexpected tool or server requests and must not expose shell, filesystem, MCP, or process handlers from a writing adapter.

## One synthetic generation

The one and only live generation used an empty V3-owned working directory under `./.local/codex-live-qualification/`, stdin input, `--ephemeral`, `--ignore-user-config`, `--ignore-rules`, `--sandbox read-only`, `--skip-git-repo-check`, JSONL output, requested model `gpt-5.6-luna`, effort `max`, service tier `priority`, and the overrides above. The prompt requested an English lantern vignette of at most 80 words, explicitly prohibited tools/filesystem/external-resource access, and required an exact ending.

The process dispatched once and completed in about 12 seconds with exit code `0`. JSONL contained `thread.started`, `turn.started`, `item.completed`, and `turn.completed`; no command, shell, MCP, file-change, function, or other tool event was observed. Stderr was empty. The returned synthetic vignette met the requested length and ending. Usage reported 6,999 input tokens, 219 output tokens, and 141 reasoning-output tokens.

The JSONL events did not report the effective model, effort, or service tier. Therefore `gpt-5.6-luna`/`max`/`priority` are exact application request values, not independently echoed upstream identity evidence. The executable version is independently verified; model and trait application remain a reported-identity gap.

Exact short evidence is retained in the ignored run directory `./.local/codex-live-qualification/run-20260905-194642-359364d1/`:

- `stdin.txt` — exact synthetic input;
- `safe-options.json` — nonsecret executable/options record;
- `stdout.jsonl` — exact JSONL response;
- `stderr.safe.txt` — sanitized stderr;
- `summary.json` — exit, timeout, tool-event, and event-type metadata.

The owned process exited and no child remained. This observed cleanup is only a single-process success; it does not qualify descendant containment or Windows Job Object kill-on-close behavior.

## Remaining W8/E3 gates

This experiment did not qualify refusal or truncation, authentication failure, broken/partial streams, Stop and cancellation linearization, process-tree cleanup under descendants, recovered terminal history, credential-entry/storage leakage, context-budget enforcement, structured-output behavior, or internal retry reporting. It also did not prove that disabled feature settings remove every available tool source. The CLI may perform provider-managed retries; V3 must expose that uncertainty and never claim exactly-once external execution or billing.

The first supported provider remains unavailable until W8's exact configuration, streamed terminal states, failure/Stop/recovery, credential, and retry evidence is complete. Manual offline writing remains independent of this qualification. See [W8 in the implementation status](IMPLEMENTATION_STATUS.md#W8--one-qualified-live-provider), [E3 in the first-slice plan](V3_FIRST_SLICE_PLAN.md#5-experiments-that-can-change-the-architecture), and the provider/lifecycle contract in [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md#11-providers-jobs-and-interruption).

Official CLI references used during qualification: [noninteractive mode](https://learn.chatgpt.com/docs/non-interactive-mode), [sandboxing](https://learn.chatgpt.com/docs/sandboxing), and the [Codex app-server lifecycle](https://learn.chatgpt.com/docs/app-server).
