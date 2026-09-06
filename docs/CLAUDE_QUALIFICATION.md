# Claude author qualification

**Status:** The Claude author development slice and its bounded native picker
and HTTP-memory fixture are qualified on the current binary. This evidence does
not qualify Claude generation, authentication, or live-provider behavior.

## Current author surface

- The picker exposes the static full IDs `claude-fable-5`, `claude-opus-5`, and
  `claude-sonnet-5` under Claude Code. Each row offers exactly `low`, `medium`,
  `high`, `xhigh`, and `max` effort, defaults to `high`, and has no service
  tier.
- Claude author requests use `claude-stdin.author.v1` through the existing
  Discuss, Propose edits, and Continue ownership and proposal Apply paths.
  The binding freezes the observed CLI version and executable fingerprint, the
  selected model, and exact effort. These are observed request facts, not a
  future version or executable hash pin; no catalog fingerprint is used.
- The application caps one serialized stdin packet at 24 KiB and retained
  provider output at 64 KiB. Claude has no Story Memory or story-lookup route;
  Luna maintenance remains an independent provider choice.
- A connection check is explicit and uses bounded native `--safe-mode`
  version/help/auth checks without generation. An unchecked connection leaves
  Claude author sending blocked. Stop and failed local saves use the existing
  recovery path and never replay a provider request.
- Schema 30 adds nullable `provider_results.reported_model`. A Completed result
  requires the reported identity to equal the requested model. Missing or
  mismatched identity becomes a failed, inspectable result retaining the safe
  raw output and both model fields. Effective identity and usage remain unknown
  when Claude counters cannot be represented by the core provider contract.

## Dated installed evidence

The earlier foundation check observed native
`C:\Users\fzwin\.local\bin\claude.exe`, Claude `2.1.220`, and executable
SHA-256 `af5bf1f1b2aadffc768eccd787084c6fdf9ba81624cbe96c1c6d9ac1a1550231`.
Its sanitized safe-mode auth probe exited with code 1 and reported
`loggedIn: false`, `authMethod: none`, and `apiProvider: firstParty`. No
credential contents or account identity were retained. This remains dated
logged-out evidence, not a live qualification.

The foundation used the installed help surface to validate print, safe mode,
empty tools, strict MCP selection, disabled sessions, explicit model/effort,
streamed JSON, partial messages, and custom system prompt controls. These are
launch controls, not proof of effective model identity, billing, isolation, or
provider behavior. Safe mode retains managed authentication and administrator
policy; persistent sessions, tools, MCP, resume, fallback, and replay remain
excluded.

The prior synthetic foundation record of 32 focused tests (12 Claude library,
12 `claude_exec`, and eight `claude_runner`) and strict workspace Clippy is
dated foundation evidence. It made no Claude LLM calls and does not qualify the
integrated desktop author route. The parser/process bounds and cleanup limits
remain documented in the [Windows process contract](WINDOWS_PROCESS_CONTRACT.md).

## Current qualification boundary

Implementation commit `1f7c761c9a292911b9f01a6faa1f91d0425fd434` passes
[CI 34043206073](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34043206073):
Windows and Ubuntu contracts, all 47 strict native checks, and the six-check
HTTP/picker fixture with six synthetic POSTs, zero live calls, and no page
errors. The final native evidence and binary fingerprint are recorded in
[implementation status](IMPLEMENTATION_STATUS.md). Further adapter ports are
deferred; Codex is the primary use case.

The native fixture passed on binary SHA-256
`563f6bfd7ba88c30711ebef7dd50da8423d7147fe4fee24bf1c4fd0aab7eb576`:
`.local/native-results/http/qualification.json` finished at
`2026-09-06T15:42:18.033Z` with six grouped checks, six synthetic POSTs, zero
live calls, zero page errors, and credential cleanup. It selected and reopened
the three Claude models with all five effort choices, kept Send blocked while
the native connection was unchecked, and preserved the independent HTTP Luna
Story Memory setting. No Claude connection or authentication probe ran.

The focused local validation passed formatting, strict workspace Clippy, the
TypeScript check, and the production build with 644 active Rust tests (589
core, 55 desktop) plus one existing ignored test. The frontend final log
`.local/claude-author-frontend-final.log` records 357 tests in 27 files after
correcting a stale usage-copy assertion; the complete wrapper was not rerun
end to end after that correction.

API-only checkpoint
[CI 34041814151](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34041814151)
is also green with dated native/HTTP evidence. These checks establish the
bounded picker/readiness and HTTP-memory surface only. Live Claude generation,
provider refusal/error/cleanup breadth, authentication, and installed-release
qualification remain open.
