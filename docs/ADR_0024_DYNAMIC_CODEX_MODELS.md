# ADR 0024 — Dynamic Codex model discovery and author bindings

**Status:** Implemented development slice; wrapper and bounded Luna/Mini
qualification pass, while broader provider and live/release gates remain open

**Date:** 2026-09-06

The fixed Codex maintenance profile remains useful, but author-facing writing
must follow the models and traits exposed by the installed native CLI. The
catalog is therefore discovered explicitly in Settings and carried into each
author request as immutable, checked metadata.

## Decision

### Explicit connection check owns discovery

Opening the picker or reading cached provider state never starts a CLI. The
author must choose **Check Codex connection** in Settings. The check selects
the installed native executable, records its observed version and SHA-256, and
runs version, login-status, and strict compatibility preflights. It then runs
the bounded interactive app-server exchange:

1. `initialize`;
2. `initialized`;
3. paginated `model/list` requests, with hidden rows excluded.

The exchange runs inside the owned Windows Job Object. Its limits are 15
seconds, 1 MiB of total output, at most 32 pages, and at most 256 models. A
discovery must finish cleanly and produce one complete, sanitized catalog;
protocol errors, duplicate or malformed rows, incomplete pages, and limit
violations fail the check. A failed or partial refresh leaves the last complete
cache in place. The observed CLI version and executable hash are provenance
metadata, never a global version or executable pin.

### The app library stores descriptive catalog state

Library schema 4 stores the sanitized model IDs, labels, declared reasoning
and service-tier traits, defaults, observed CLI identity, and discovery time
under the app-owned `app_preferences` catalog key. It stores no credentials,
auth state, or request text. Cached rows are descriptive evidence and do not
establish dispatch readiness. The current native connection must authorize the
exact selected model and concrete traits before a request can start.

If a refresh removes a saved model or trait, the saved selection remains
visible as unavailable. The picker never silently substitutes another model or
trait. A newly selected unavailable combination is rejected until the author
checks a connection that declares it.

### Author requests bind the resolved catalog entry

An author-facing request uses `codex-stdin.author.v1`. Its binding contains the
concrete resolved model, reasoning level, and service tier; observed CLI
version and executable hash; and the SHA-256 of the sanitized model descriptor
in `runtime.catalogSha256`. It retains the existing application byte limits:
24 KiB input and 64 KiB output. These are local byte caps, not inferred model
context or token limits.

The checked connection owns a catalog snapshot. Starting a request clones the
connection state and freezes the binding, so a later Settings refresh cannot
change an in-flight request. A refreshed descriptor that no longer matches
cannot replay the old binding. Existing saved results remain inspectable. The
renderer receives an explicit request acknowledgment; lost UI acknowledgment
does not automatically start a second generation. Historical failed receipts
retain their original provider binding and outcome; qualification artifacts
separately record the tested native binary. A transport fix is checked with a
new explicit request while earlier failures remain unchanged.

### Compatibility and routing boundaries

Legacy `codex-stdin.v1` bindings, including historical 0.153.3 packets, keep
their exact serialized bytes and hashes. Project schema 27 raises the reader
floor because author bindings have a distinct validation profile; it changes
no project tables. Earlier packet bytes and the fixed maintenance profile are
preserved.

Summary, story-memory, and other maintenance work continues to use
GPT-5.6-Luna with `xhigh` reasoning and the `priority` service tier. The
selected writing model never redirects maintenance work; native state exposes
a separate `memoryReady` result for that fixed route. Manual editing and the
OpenAI-compatible HTTP path retain their existing contracts.

HTTP context lookup and wider V2 CLI adapters remain pending. This decision
also does not qualify broader live providers, general provider parity,
narrative quality, installed-release behavior, or release readiness. Those
gates remain open.

## Qualification boundary

The detailed implementation and evidence ledger remain in
[implementation status](IMPLEMENTATION_STATUS.md) and
[Codex qualification](CODEX_QUALIFICATION.md). The preceding HTTP checkpoint at
source `7ce8b76` reports native CI run `34033575745` passing the 46 strict
checks and HTTP fixture, while the overall run failed Ubuntu Clippy because of
a Windows-only helper. That helper is fixed in the current tree; newer CI is
pending. The post-parser-fix wrapper passes 589 active Rust tests (548 core and
41 desktop), one existing ignored fixture, and 350 frontend tests in 27 files,
with formatting, strict Clippy, TypeScript, and the production build. The parser
fix accepts nullable `defaultServiceTier`, valid non-text audio modalities
(excluding audio-only rows), and the sanitized 0.153.4 seven-model fixture.
Current native discovery found seven models. The first Luna/xhigh/priority
request completed; an inherited Luna-only Responses-Lite route initially caused
Mini/low to fail. Since `model/list` does not declare that route, the profile
uses standard Responses for other models, without substitution. The explicit
Mini/low/no-tier follow-up completed after that fix, so bounded Luna/Mini
qualification passes. Broader provider, HTTP-live, and release gates remain
open.
