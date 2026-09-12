# ADR 0003 — Recent discussion context

**Date:** 5 September 2026

**Status:** local implementation; native and full verification recorded in [implementation status](IMPLEMENTATION_STATUS.md). C3 remains partial.

## Decision

Follow-up author-room discussions receive a bounded selection of earlier complete exchanges from the same document's current project thread. Each exchange retains the exact author question, original selected scope if any, completed assistant reply, message/run IDs, source snapshot, and policy version.

This is conversation evidence. It does not adopt earlier suggestions, activate old instructions, or establish story facts. The final author request and explicitly adopted [guidance](ADR_0002_AUTHOR_GUIDANCE.md) retain instruction authority. No paid summarization or extra planning invocation is introduced.

The first selector uses recency, not semantic relevance. It considers at most the four most recent completed and delivered turns under the current policy and operation namespace. It includes whole turns up to 16 KiB of their exact serialized representation, including selected quotations. It reads size metadata before loading a large reply. On reaching the size limit, it stops before that turn; whitespace-only replies are skipped. These are initial local-work limits, not model-quality findings.

Queued, running, stopped, failed, and interrupted output is not supplied as a complete exchange. The original discussion still retains that material. Another document, an older policy, another project, and copied historical threads are excluded from automatic selection. Restricted-writing requests receive no conversation context.

## Frozen data and receipts

Reuse the existing immutable discussion messages. `FrozenContext.conversation` stores the bounded exact selection beside document sources and adopted guidance. No additional table or independently editable memory copy is needed: discussion messages are already retained and protected against deletion. Future pruning must preserve these references.

The discussion-start transaction freezes this selection with the request, source revisions, packet, author message, and job. A repeated start operation returns the same packet even if another response has since completed. Conversation output does not advance manuscript freshness or rewrite an old snapshot.

Reads validate message identity, content, role, scope, thread, run completion, project/namespace, and source packet relationships. Selected scopes are checked against their original document revision. Validation visits direct retained records rather than recursively recompiling a chain of earlier conversation packets.

The envelope records the complete supplied turns in chronological order. The receipt separately records their ordered message IDs and the number of eligible completed turns omitted by the selector or input budget. This count does not claim coverage of stopped replies or disallowed conversation. The inspector shows earlier exchanges separately from guidance and story sources, with the exact frozen text and any omissions.

Empty optional fields are omitted so existing snapshots and packets retain their serialized form and fingerprints. Exact packet validation still reproduces the stored input under its recorded contract; it does not replace retained history with a newly chosen conversation.

## Budget priority

First try the complete eligible story and selected conversation together. If that fits, include both without manufacturing excerpts or summaries.

Otherwise preserve the complete target, current instruction, editable scope, mandatory source/dependency closure, and adopted guidance. Then add a stable prefix of whole recent exchanges, followed by the existing stable prefix of optional story blocks. Stop at the first optional item that cannot fit. This means a small budget may leave room unused rather than drop an earlier selected item to squeeze in a later one. More budget can extend the same selection without replacing evidence already supplied.

A prior exchange is never shortened or separated into half a turn. A mandatory overflow still refuses preparation. Pinning an older story source makes it mandatory before optional conversation. This initial priority recipe is a deterministic engineering baseline; its literary usefulness remains an evaluation question.

## Recovery and remaining work

Policy revocation blocks request-facing access to an old packet while preserving valid historical records in backups. A recovered project retains its copied discussion for the author to read, but automatic context begins with the recovered project's new thread. The author can explicitly keep a historical message as guidance.

Explicit unchanged retries of unsuccessful attempts now retain eligible original Next request guidance under [ADR 0002](ADR_0002_AUTHOR_GUIDANCE.md), while compiling fresh story and recent-discussion context. Persistent chapter/project source pins, explicit safe briefs, richer conversation selection, durable Apply, digests, and live-provider lookup remain separate work. No provider session is needed to reconstruct this local discussion context.
