# ADR 0025 — Independent API provider for story memory

> **8 September 2026 update:** new summary and maintenance requests now use
> GPT-6 Astra with low reasoning, independently of the author model picker.
> Codex requests priority; HTTP omits service tier. Historical Luna bindings
> remain readable and are never rewritten. The original contract below records
> the earlier model choice; this update supersedes that choice only.

**Status:** Implemented development surface; accepted native synthetic evidence exists,
while hosted/live-provider and release qualification remain open.

**Date:** 2026-09-06

Story Memory is an explicit, app-owned operation. Its provider preference must
not follow the author's writing-model picker, and saving or autosaving a
manuscript must never send a memory request.

## Decision

### Independent preference and provider choices

The app stores the Story Memory provider under the separate `app_preferences`
key `story-memory-provider-v1`. Updates use the app-preferences CAS revision.
The preference is independent from the author-facing model selection.

The initial choices are:

- **Codex (default):** the native fixed Luna/xhigh route with the `priority`
  service tier.
- **Mock:** an explicit offline choice for deterministic local development.
- **Configured HTTP:** a user-selected OpenAI-compatible endpoint. The route
  always requests Luna with xhigh reasoning; the user must choose an endpoint
  that supports those settings. HTTP has no service-tier setting.

An absent or malformed provider configuration, missing model, or missing or
unreadable key blocks the job with an exact error. The app does not fall back to
another provider, model, endpoint, or tier. Fixed request settings describe the
request; they do not prove the effective model or reasoning behavior of the
remote provider.

### Frozen job binding

At job acceptance, the native route freezes the selected route and configuration
revision and captures the credential privately in the adapter. The Story Memory
binding persists no credential reference or key. Later Settings changes cannot
retarget an accepted job. The local result, terminal outcome, and reconciliation
state are owned by the existing Story Memory job lifecycle.

The HTTP binding is `openai-chat-completions.memory.v1`. It permits at most
2 MiB of prepared input and 64 KiB of retained output. These are application
byte limits, separate from provider token limits and billing. HTTP author
lookups remain unsupported; this route is for Story Memory only.

### Delivery and persistence

HTTP memory requires a delivery receipt. Schema 29 adds
`memory_results.delivery_json`; it records the prepared body and delivery state
without changing the memory result's semantic content. Existing nullable
delivery values remain SQL `NULL`/omitted when absent, and previous packet and
result bytes are preserved; legacy records are not reconstructed or rewritten.

Local result saving and reconciliation never replay a POST. An uncertain or
failed local save can settle through the local recovery path, but only an
explicit new memory action may submit a new request. No paid memory call runs
on autosave, opening a panel, or ordinary manuscript persistence.

## Boundaries and remaining work

Live maintenance uses the fixed Luna/xhigh contract through Codex with priority
or through the selected HTTP endpoint with no service tier. The mock option is
explicitly offline. The independent preference,
configured HTTP route, private credential capture, schema-29 persistence, and
local result reconciliation are implemented development behavior. Accepted
native HTTP evidence uses synthetic endpoints and does not establish hosted or
live-provider support. A configured endpoint or a stored preference alone is not
live provider evidence. Broader provider support, HTTP author lookups, and
release qualification remain separate gates.
