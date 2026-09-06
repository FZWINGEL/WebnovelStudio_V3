# ADR 0026 — Bounded Claude author provider

**Status:** Implemented native development surface; bounded picker/readiness
and independent HTTP-memory fixture pass on the current binary. Claude
generation, authentication, and live-provider qualification remain open.

**Date:** 2026-09-06

## Decision

Claude is an explicit author-facing provider selected through the existing
model picker. It is independent from Story Memory and story lookup. Luna
maintenance remains on its existing provider route; Claude cannot silently
become a maintenance or lookup provider.

The static reference catalog contains exactly these full model IDs:

- `claude-fable-5`
- `claude-opus-5`
- `claude-sonnet-5`

Each model exposes `low`, `medium`, `high`, `xhigh`, and `max` effort, defaults
to `high`, and has no service tier. Settings owns the author choice. Sending
requires an explicit native Claude connection check; an unchecked or unavailable
connection leaves the author request blocked.

## Frozen request contract

The author binding is `claude-stdin.author.v1`. Acceptance freezes the exact
model and effort plus the observed CLI version and executable fingerprint. The
identity is recorded as request evidence only: there is no future version pin,
executable hash allowlist, or catalog fingerprint. The native launch uses
safe-mode, an app-owned system instruction, disabled sessions, no tools/MCP,
and one owned Windows process tree.

The exact serialized packet is sent over stdin with a 24 KiB application input
cap. Retained provider output is capped at 64 KiB. Discuss, Propose edits, and
Continue use the existing core discussion ownership, proposal records, and
explicit Apply path. Stop and failed local saves use local recovery; neither
operation replays or automatically resubmits a Claude request.

## Identity and persistence

Schema 30 adds nullable `provider_results.reported_model`. A Completed result
is accepted only when the provider-reported model exactly matches the frozen
requested model. Missing or mismatched identity becomes a failed, inspectable
result while retaining safe raw output and both requested/reported fields.
Effective provider identity and usage remain unknown when Claude's optional
counters cannot be represented by the existing all-required Codex usage
contract. Historical rows without `reported_model` remain nullable.

The connection check runs bounded native version, help, and authentication
probes and never generates text. Safe mode still observes managed
authentication and administrator policy. Claude author requests do not expose
Story Memory or `story-lookup.v1`.

## Qualification boundary

The integrated source checkpoint passes Windows/Ubuntu contracts and all strict
native checks in [CI 34043206073](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34043206073).
This includes the blocked Claude picker and synthetic HTTP routes; no live
Claude generation is qualified. The author has deferred further adapter ports
and selected Codex as the primary use case.

The prior installed evidence is dated logged-out evidence with zero Claude live
calls. The current native fixture passed on binary SHA-256
`563f6bfd7ba88c30711ebef7dd50da8423d7147fe4fee24bf1c4fd0aab7eb576`;
`.local/native-results/http/qualification.json` finished at
`2026-09-06T15:42:18.033Z` with six grouped checks, six synthetic POSTs, zero
live calls, zero page errors, and credential cleanup. It verified the three
Claude model rows, five effort choices, save/reopen behavior, blocked Send
while unchecked, and independent HTTP Luna Story Memory selection. No Claude
connection or authentication probe ran. API-only checkpoint
[CI 34041814151](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34041814151)
is also green with dated native/HTTP evidence. Focused local validation passed
formatting, strict workspace Clippy, TypeScript, and the production build with
644 active Rust tests (589 core, 55 desktop) and one existing ignored test;
the frontend final log records 357 tests in 27 files after a stale usage-copy
assertion was corrected. The complete wrapper was not rerun end to end. These
checks qualify only the bounded native picker/readiness and HTTP-memory surface;
live Claude generation, authentication, broader refusal/error/cleanup coverage,
and installed-release qualification remain separate gates.
