# ADR 0023 — Configurable API connections

**Status:** Implemented development slice; executed qualification is recorded in
[implementation status](IMPLEMENTATION_STATUS.md). This does not qualify every
service that describes itself as OpenAI-compatible.

The author can configure several connections in Settings, then select their
models from the persistent picker. The picker retains V2's provider browsing,
cross-provider search, favorites, keyboard selection, and separate model traits.
Browsing or opening Settings does not contact an endpoint. **Find models** is an
explicit, cancellable read; manual model IDs work when discovery is unsupported.

## Connection and credential ownership

The app-local library, schema 3, owns each connection's stable ID, label, base URL,
enabled flag, JSON-mode choice, manual/cached model IDs, and configuration revision.
An endpoint ID is also its provider ID, so identically named models on different
servers remain distinct. A route change clears discovered models. Removed models
remain unavailable selections when needed to preserve a saved choice or favorite;
the app never silently chooses another model.

The field accepts an API **base URL**, for example `http://localhost:1234/v1` or
`https://gateway.example/api/v1`. The adapter appends `/models` for discovery and
`/chat/completions` for responses. A bare origin gets `/v1`; a supplied base path
is preserved. An exact completion URL is not a base URL. HTTP and HTTPS are
supported, including local/LAN servers; userinfo, query strings, fragments, and
redirects are rejected. The optional key is sent as bearer authorization.

Windows Credential Store owns key values. SQLite retains an opaque app-owned
credential reference, while renderer reads receive only availability metadata.
Keys are not exported into projects, request packets, histories, diagnostics, or
provider result records. An absent or unreadable referenced key blocks dispatch;
it never changes an authenticated profile into an anonymous connection. Keyless
connections are an explicit setup choice. Other OS credential backends remain
unqualified.

Rotation first creates a new credential, then publishes its reference with the
library revision check. A read after a failed acknowledgement decides whether a
new reference was published; an unknown save never causes blind replay or deletion
of a possibly active credential. Superseded credentials are deleted only after a
read proves that no profile references them. Cleanup failures are surfaced.

## Request and settlement ownership

The native runtime captures the active model, endpoint configuration revision,
route, and key before accepting a new discussion. A running request retains its
captured connection when Settings change. Existing operation reconciliation uses
the persisted binding without loading a new key or issuing another network call.
If acceptance committed but its acknowledgment was lost before any worker claim,
reconciliation respects a registered worker or atomically claims and seals the
orphaned request as failed with `notSent`. It does not leave the request queued
forever or reconstruct an old credential. An explicit new request can try again;
a failed local receipt write uses the existing save-recovery path. Reopening a
project interrupts any remaining queued/running operations without replay.

The binding has profile `openai-chat-completions.v1` and an optional HTTP section.
Legacy Codex packets omit that section and retain their exact historical bytes and
hashes. Codex version checks and Luna/xhigh maintenance routing are unchanged.

Rust serializes the exact chat-completion body once through a pure preparation
function; the transport sends those bytes. The body includes the frozen messages,
chosen model, streaming flag, and explicitly enabled JSON mode for edit requests.
Discovery alone does not establish supported reasoning, service tiers, tool
calling, or token limits. The initial endpoint catalog exposes no invented traits.

Project schema 26 adds an optional delivery receipt to existing provider results:

| Receipt | Meaning |
| --- | --- |
| Body hash and bytes | Exact prepared HTTP request body; separate from the neutral packet hash |
| `notSent` | The transport did not begin submission |
| `uncertain` | Submission began; the app cannot establish whether the endpoint received it |
| `responseReceived` | Response headers arrived; the body may still be partial or invalid |
| Optional usage fields | Provider-reported token counts, with missing values remaining unknown |

HTTP leaves the legacy `confirmed_stdin_bytes` field at zero. A response header is
not proof of model comprehension, input consumption, upstream cancellation, or
billing. Local cleanup completion describes the local HTTP worker only.

The adapter bounds requests, response bytes, SSE lines, and elapsed time, rejects
malformed/refused/tool responses, and preserves partial text on transport failure
or Stop, including text carried in the same frame as a truncation finish reason.
Stop wins over a late successful response. Network calls are never
automatically replayed; only local durable settlement can be retried. Completed
typed suggestions use the existing scope validation, Preview, and atomic Apply
path. Failed or incomplete responses cannot become applicable suggestions.

## Initial scope and remaining work

This route supports ordinary discussion, selected/structured suggestions, and
continuation through the existing context and proposal contracts. JSON mode is
opt-in because compatible servers differ. There is no automatic fallback after
an endpoint rejects the chosen format or model.

The first HTTP slice excludes the bounded story-lookup protocol. The UI exposes
that limitation and Rust rejects unsupported requests. Explicit story-memory
jobs continue to use GPT-5.6-Luna/xhigh through Codex; no analysis runs on autosave.
Remaining V2 CLI adapters and live model discovery need their own native transport
and capability qualification. A model appearing in a reference catalog does not
establish that its adapter works.

Local mock HTTP qualification, native Windows qualification, authenticated live
service behavior, and narrative quality remain distinct evidence gates. Tests
must use synthetic projects and keys, and remove their own test credentials.
