# ADR 0002 — Author guidance records

**Date:** 5 September 2026

**Status:** accepted design; local implementation is present in the C3 discussion path, with final qualification and remaining C3 integrations still open.

This ADR defines the C3 author-guidance record used by the Story Context Engine. It extends the source and policy contracts in [V3_STORY_CONTEXT_SYSTEM.md](V3_STORY_CONTEXT_SYSTEM.md), the first-slice plan in [V3_STORY_CONTEXT_FIRST_SLICE.md](V3_STORY_CONTEXT_FIRST_SLICE.md), and the native/editor boundaries in [ADR 0001](ADR_0001_EDITOR_CONTRACT.md). It does not make guidance a second manuscript, a hidden note, or canon.

## Decision

Author guidance is stored as immutable `author_guidance_versions` with mutable `author_guidance_heads`. Each edit creates a new version; retiring guidance advances the head to a retired state. The record has an explicit scope:

| Scope | Meaning |
| --- | --- |
| `request` | Apply to the next explicit discussion for the current document. It is consumed only when that new logical request successfully compiles and binds its frozen snapshot in the start transaction. Linked retries do not yet reuse consumed request guidance. |
| `document` | Apply to discussions for one document until the author edits or retires it. |
| `project` | Apply to discussions in the project until the author edits or retires it. |

The exact editable guidance text is author-owned. A record may retain an optional originating conversation message for provenance, but provenance never substitutes for the author-confirmed text. Directly authored instructions use the same versioned record and do not require a chat message.

Guidance is distinct from:

- the current manuscript and its document revisions;
- a generated assistant suggestion;
- an accepted story rule or reviewed story record; and
- raw discussion history.

Only an explicit author action can create, edit, retire, or later promote a guidance record into another authority category. Repeating an assistant suggestion in chat does not activate it.

## Snapshot and packet binding

A frozen context snapshot stores the selected guidance's exact text, content hash, version ID, and record reference alongside ordinary document `SourceRef` values. Guidance is not represented by pretending that it is a document revision. The selected text and hash are mandatory packet input. If mandatory guidance cannot fit the authorized budget, preparation fails with the existing mandatory-context overflow result; the compiler does not shorten or silently omit it.

Request-scoped guidance is selected and marked consumed in the same start transaction that successfully compiles and binds the discussion snapshot. Consumption does not bump `context_source_epoch`, because the guidance was already selected in that request's frozen basis. Creating, editing, or retiring any guidance does bump the project context epoch and therefore makes later preparation refresh its basis. A failed compile, a changed payload, or an abandoned attempt does not consume request-scoped guidance. A linked retry is currently a new logical request; reuse of consumed request guidance is future work.

Generic packet preparation rejects a new operation using a discussion snapshot that contains request-scoped guidance. Reading the original receipt or retrying the original discussion operation remains idempotent; neither creates another use.

Stored packets are checked against the exact frozen revisions and original preparation request, including delivered sources, mandatory dependencies, guidance, instruction, options, and receipt. The current fixed mock compiler reproduces that packet for comparison; validation never substitutes a new packet for retained history. Future compiler versions must preserve the recorded packet contract. Data-integrity validation is separate from present-day permission to read or dispatch: revocation blocks request-facing access while allowing valid historical packets to remain in a backup.

The ordinary context freshness rules still apply. A source edit, policy change, or guidance edit makes a new request use a new snapshot. Existing packets remain historical and inspectable according to policy; they do not become current merely because their quotation or guidance text is unchanged.

## Author-room and writing policy

New guidance defaults to `AuthorRoom` access. It is available to author-room discussion and planning requests, subject to the request's scope and current project identity. The C3 implementation excludes all current guidance from restricted prose-writing requests. Safe-brief creation is future work; relabeling the same guidance or changing a packet label does not grant restricted access.

Safe-brief creation remains later work. That brief will be a separate, explicit transfer with its own source references, exact text, policy, and review boundary. A restricted request must never inherit private author-room guidance or privileged planning conversation merely because both requests use the same project.

## User interaction

“Keep as guidance” pre-fills the guidance editor from a selected user or assistant message. The current UI requires an explicit author Save after editing and offers Next request, This document, or This project scope. It also supports creating an instruction directly, without first generating or quoting an assistant message. Editing creates a new version; retiring advances the head to an inactive state while preserving history. The UI may show the originating message as provenance.

The confirmation explains the resulting scope and whether the instruction is author-room-only. Guidance controls must not imply that the assistant authored or reviewed the instruction.

Bounded recent conversation compilation is implemented separately in [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md). Raw chat remains locally retained; only selected complete exchanges from the current document thread and policy enter later AuthorRoom packets. Chat is not canon, does not become guidance automatically, and does not cause a source-epoch change merely because it is retained.

## Persistence, recovery, and compatibility

Guidance records belong to the project database, alongside authored documents. Recovery copies those records without an original-project-ID filter, so adopted instructions remain available in the independent recovered project. New snapshots bind them to the recovered project's identity and operation namespace. Copied historical conversations remain readable, while old operation receipts and snapshots cannot authorize new writes in the recovered project.

The serialized representation uses backward-compatible defaults and omits empty optional fields. Readers must treat an absent optional provenance or scope-adjacent field as its documented default. Writers must preserve the existing canonical field ordering and skip empty optional values. The old empty guidance snapshot/receipt JSON remains hash-compatible when no guidance was selected, while selected guidance is hashed from its exact explicit text, version, and reference. A schema or canonicalization change that would alter an existing hash requires an explicit migration and evidence; it must not be smuggled in as a nullable field change.

Guidance mutation and snapshot binding are Rust-owned persistence operations. JavaScript may own the live editor and form state, but it cannot claim that guidance was adopted until Rust has accepted the exact record and returned its receipt. The packet carries frozen guidance as separately typed mandatory AuthorRoom input with distinct guidance handles for the receipt and inspector. Guidance updates never replace editor content and never authorize Apply.

## Consequences and boundaries

This design gives the author a durable place for decisions such as “keep the ending” or “make Mei less sarcastic” without turning every conversation into permanent story truth. Versioning preserves provenance and supports inspection, while the explicit scope prevents a one-off instruction from silently becoming a project-wide rule.

The implementation remains deliberately small: relational records, exact text and hashes, existing snapshot/packet receipts, and the current context epoch. It does not add hidden notes, a second canon database, an independently editable memory graph, provider-side memory, or a new orchestration framework.

The local implementation and tests cover request consumption after successful binding, edit/retire epoch invalidation, mandatory-budget refusal, restricted-writing exclusion, recovery identity fencing, exact packet receipts, and stable empty-field serialization. The native guidance flow also passes. Richer conversation selection, linked retry guidance reuse, persistent pins, safe briefs, and release qualification are separate work; this ADR does not claim a shipped release.
