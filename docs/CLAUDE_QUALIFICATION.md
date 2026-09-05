# Claude provider qualification

**Status: candidate only, 5 September 2026.** No Claude generation request has been made, and no Claude provider is enabled in the app.

## Installed executable evidence

The direct native executable `C:\Users\fzwin\.local\bin\claude.exe` reports Claude Code `2.1.220`. Its SHA-256 is `af5bf1f1b2aadffc768eccd787084c6fdf9ba81624cbe96c1c6d9ac1a1550231`. Discovery used `--version`, `--help`, and `auth status --help`. A filtered local `--safe-mode auth status --json` check reported `loggedIn=false`, `authMethod=none`, and `apiProvider=firstParty`. No credential contents or account identity fields were displayed or retained.

Installed help exposes print mode, safe mode, empty built-in tool selection, skill disabling, strict MCP selection, custom system prompts, no session persistence, explicit model/effort, streamed JSON, and partial messages. This makes the version a candidate for W8/E3 testing. Parsing flags and reading help do not establish their effective runtime behavior.

## Version and capability boundary

The current [CLI reference](https://code.claude.com/docs/en/cli-reference) documents that safe mode retains authentication but still permits managed policy, including policy hooks. An empty `--tools` disables built-ins; MCP needs its own restrictions. The newer documented `--restricted` option requires `2.1.248`, so it is unavailable to the inspected executable. Do not assume current web documentation describes every behavior of `2.1.220`.

The [programmatic-use guide](https://code.claude.com/docs/en/headless) documents newline-delimited JSON output and requires verbose mode with partial messages for streaming. Bare mode does not use subscription login. These documents motivate the experiment; they do not qualify model identity, isolation, token limits, cancellation, or billing.

## Implemented protocol foundation

The pure `ClaudeJsonlParser` accepts incremental stdout chunks and emits validated assistant text. Twelve synthetic tests cover UTF-8 chunk boundaries, initial block text, cumulative stream usage, usage-only deltas, result-counter regression, terminal refusal/truncation, tool events, session/message identity, malformed data, and bounded output. Stream and result usage are retained separately; absent usage stays unknown. Initial and final visible text must agree, and an unexpected tool or provider error cannot become a successful response. Thinking blocks are discarded rather than displayed. Fixed failure descriptions retain only validated partial assistant text.

The decoder limits individual lines to 1 MiB, total input to 16 MiB, events to 8,192, blocks to 128, and observed text to 512 KiB. Full-message confirmations count again toward observed text: this conservative wire limit is not a promise of 512 KiB of unique generated text. The selected model must still be compared with the reported model by the eventual adapter. Metadata checks do not establish effective managed-policy isolation.

The [Windows process primitive](WINDOWS_PROCESS_CONTRACT.md) now observes bounded output incrementally and retains its cleanup contract. Neither component is connected to production dispatch. The parser is tested against synthetic records informed by the official [streaming contract](https://platform.claude.com/docs/en/build-with-claude/streaming), whose message usage counters are cumulative, and the [Python SDK types](https://github.com/anthropics/claude-agent-sdk-python/blob/b1b838b1c5730a7a0b270915a79b15861a8ca716/src/claude_agent_sdk/types.py). A real CLI transcript still needs qualification.

## Next bounded experiment

Connect the tested process observer and decoder only after defining the exact launch and usage contract. Keep manuscript packets on stdin, use an application-owned empty working directory, and explicitly choose the environment. Do not port V2's shell launcher, unrestricted tool permissions, implicit catalog limits, or fallback-to-mock behavior.

The candidate configuration must request safe mode, no built-in tools, no MCP tools, disabled skills, no Chrome connection, no persisted provider session, explicit model/effort, a fixed application system prompt, and JSONL partial output. Do not enable automatic fallback. Require the exact qualified executable/version before launch. A decoder must refuse unexpected tool/configuration events, preserve valid partial text, distinguish reported from requested model information, and retain unknown usage as unknown.

Authentication is currently unavailable. Implement and test the transport/protocol with synthetic fixtures before any authenticated generation trial. A later trial needs a signed-in supported CLI or an explicitly configured HTTP credential. A successful text response alone would still leave the named W8/E3 failure and native setup gates open.

The first supported adapter remains a qualification choice, separate from the author's saved model preference. Codex remains a distinct candidate with its own [evidence record](CODEX_QUALIFICATION.md). No model or provider may be silently substituted.
