# ADR 0027 — AI writing workspace and human-reviewed generation

**Status:** Implemented development slice with schema-31 migration, local contracts, native regression, and one bounded live Codex draft; broader release and author qualification remain open.

**Date:** 2026-09-06

## Context

AI writing in the desktop workspace is user-directed. The author chooses the
working document and the action, then reviews the returned material before it
can change project content. Opening a project, changing tabs, or restoring a
saved preference must not submit an LLM request.

## Decision

Each project exposes five local writing areas: **Chapters**, **Worldbuilding**,
**Characters**, **Plot & themes**, and **Notes**. The selected tab and last
document ID are stored per project and per tab. Chapters share their ordered
chapter list with the editor and its Previous/Next navigation. Changing tabs or
documents flushes and detaches the current editor before the next document is
mounted; an empty tab does not display content from another category.

The primary writing actions are **Draft** for a chapter, **Continue** for an
existing chapter, and **Develop** for a permitted nonchapter document. A normal
new generation requires an explicit **Send**. Returned material remains a
candidate or proposal for inspection, preview, and editing where supported;
the author must explicitly **Apply** or reject it. There is no automatic LLM
request from navigation, startup, readiness, or preference restoration.

The picker exposes the supported reasoning and service-tier choices inline.
For a fresh revision-0 app state with the mock choice active, or for an app
with a saved Codex choice, a startup Codex check may perform a read-only
compatibility check. A successful check adopts Luna with `xhigh` reasoning and
`priority` service tier only when the writing choice is still untouched;
explicit user selections remain unchanged. The check itself never generates.

Structured development is scope-gated to the author room and a permitted,
author-only current working nonchapter target. Chapter prose remains
restricted; author-room material cannot widen a chapter request or establish
canon by itself. The schema-31 minimum reader fence prevents older readers from
opening new author-room development records as supported data.

## Consequences and verification boundary

Readiness and intent remain visible without silently changing the saved model
or sending a request. Candidate persistence and Apply remain separate actions.
Current executed checks and the bounded native/live evidence are recorded in
[implementation status](IMPLEMENTATION_STATUS.md). They do not establish full
provider reliability, narrative quality, or author acceptance.

Related: [model preferences](ADR_0010_MODEL_PREFERENCES.md), [bounded Codex](ADR_0011_LIVE_CODEX.md), and [dynamic Codex models](ADR_0024_DYNAMIC_CODEX_MODELS.md).
