# Claude foundation qualification

**Status:** The text-only foundation is implemented and synthetic-tested. Claude is not
enabled in the model picker or desktop dispatch. No live Claude support or provider
qualification is claimed.

## Installed executable evidence

- Native executable: `C:\Users\fzwin\.local\bin\claude.exe`, Claude `2.1.220`;
  observed SHA-256 is `af5bf1f1b2aadffc768eccd787084c6fdf9ba81624cbe96c1c6d9ac1a1550231`.
- Discovery used `--version`, `--help`, and `auth status --help`. A sanitized
  `--safe-mode auth status --json` probe exited with code 1 and reported
  `loggedIn: false`, `authMethod: none`, and `apiProvider: firstParty`.
  No credential contents or account identity was retained; no Claude LLM calls
  were made.
- The installed CLI is a static, version-gated reference candidate. Catalog and
  readiness integration are not implemented. The [CLI reference](https://code.claude.com/docs/en/cli-reference)
  and [headless guide](https://code.claude.com/docs/en/headless) describe flags and
  JSONL behavior; they do not qualify effective isolation, billing, or model identity.
  Safe mode retains authentication and managed policy; the documented `--restricted`
  option requires a newer CLI than `2.1.220`.
- Installed help exposes print and safe mode, empty built-in tools, skill
  disabling, strict MCP selection, custom system prompts, disabled sessions,
  explicit model/effort, streamed JSON, and partial messages. These are available
  controls, not proof of effective runtime behavior.

## Foundation contract

- The author profile uses a fixed app-owned instruction and sends story text over
  stdin, with Fable 5, Opus 5, and Sonnet 5 model identifiers.
- Effort is exact. Application limits are 24 KiB input, 64 KiB retained response
  text, and 4 MiB on the wire.
- The connection holds observed CLI version and executable fingerprint in memory;
  durable per-request binding is a remaining gate. There is no future version pin.
- Each request owns a Windows Job tree, including child processes. Safe mode
  disables user customizations, while administrator-managed policy and
  authentication still apply. Persistent CLI sessions are disabled. Tools, MCP,
  resume, fallback, and replay are excluded.
- A requested-versus-reported model mismatch is refused. Managed policy and
  authentication remain outside the app-owned safe-mode boundary.
- The parser/process foundation rejects malformed or unexpected events, preserves
  validated partial text, and keeps unknown usage unknown. The [Windows process
  contract](WINDOWS_PROCESS_CONTRACT.md) records the process ownership boundary.

## Synthetic evidence and cleanup

The focused synthetic result is 32 tests: 12 Claude library cases, 12 existing
`claude_exec` cases, and eight `claude_runner` cases. Workspace strict Clippy also
passes. These checks do not establish a real CLI transcript or provider result.
The parser fixtures follow the official [streaming contract](https://platform.claude.com/docs/en/build-with-claude/streaming)
and [Python SDK types](https://github.com/anthropics/claude-agent-sdk-python/blob/b1b838b1c5730a7a0b270915a79b15861a8ca716/src/claude_agent_sdk/types.py).
The decoder retains its existing bounds of 1 MiB per line, 16 MiB total input,
8,192 events, 128 blocks, and 512 KiB observed text; repeated full-message
confirmations count against observed text.

`Drop` removes only an empty temporary directory; unexpected artifacts are retained.
`cleanup_settled` means process settlement, not guaranteed file removal. Model
binding, durable catalog/readiness integration, picker integration, desktop dispatch,
live Claude qualification, refusal/error/cleanup breadth, and installed-release
qualification remain open.
