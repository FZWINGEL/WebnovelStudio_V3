# V3 documentation index

Start with the product requirements and [implementation status](IMPLEMENTATION_STATUS.md). The native development app has persistent writing, project management, discussion, exact context inspection, adopted guidance, saved sources, optional approved writing briefs, selected-passage Apply/Reject, history, export previews, bounded Codex integration, and schema-8 V2 import. Reviewed-story authority, richer memory, broader providers, and full release qualification remain open. Design documents describe the target; source and executed checks establish current behavior.

| Read | Document | Responsibility |
| --- | --- | --- |
| 1 | [Product requirements](../PRODUCT.md) | Author experience and explicit product boundaries |
| 2 | [Design surface](../DESIGN.md) | Built W0 writing canvas, feedback panel, and visual constraints |
| 3 | [Editor contract](ADR_0001_EDITOR_CONTRACT.md) | W0 snapshot, identity, scope, canonicalization, and replacement rules |
| 4 | [First-slice plan](V3_FIRST_SLICE_PLAN.md) | W0 status and the dependency order for W1 onward |
| 5 | [Workspace plan](V3_WORKSPACE_PLAN.md) | Repository, toolchain, data separation, and wrapper commands |
| 6 | [Refined architecture](V3_ARCHITECTURE_REFINED.md) | Later persistence, lifecycle, recovery, provider, and story contracts |
| 6a | [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) · [first slice](V3_STORY_CONTEXT_FIRST_SLICE.md) | Adopted context architecture and ordered C0–C6 work; preserves save, Apply, and authority ownership |
| 6b | [Author guidance ADR](ADR_0002_AUTHOR_GUIDANCE.md) | Guidance records, scopes, receipts, packet binding, and policy boundary |
| 6c | [Recent discussion ADR](ADR_0003_DISCUSSION_CONTEXT.md) | Complete-turn selection, exact frozen messages, budget priority, omissions, and policy boundary |
| 6d | [Proposal and Apply ADR](ADR_0004_PROPOSAL_APPLY.md) | Explicit feedback intent, restricted proposal context, immutable review records, and atomic author Apply/Reject |
| 6e | [Document history ADR](ADR_0005_DOCUMENT_HISTORY.md) | Saved-version comparison and explicit restore development contract |
| 6f | [Draft export ADR](ADR_0006_DRAFT_EXPORT.md) | Exact frozen Markdown/TXT previews, explicit native save, immutable export records, and partial-failure boundary |
| 6g | [Discussion source pins ADR](ADR_0007_DISCUSSION_SOURCE_PINS.md) | Persistent source choices, explicit confirmation, mandatory packet binding, retry and recovery boundaries |
| 6h | [Writing brief ADR](ADR_0008_WRITING_BRIEF.md) | Optional author-approved directions, exact restricted transfer, draft persistence, and approval/reset rules |
| 6i | [Discussion recovery ADR](ADR_0009_DISCUSSION_RECOVERY.md) | Cleanup-owned Stop, retained partial output, failed local writes, and explicit recovery without generation replay |
| 6j | [Model preferences ADR](ADR_0010_MODEL_PREFERENCES.md) | Persistent native selector, separate traits, confirmed settings, and preference persistence and historical mock binding; extended by ADR 0011 |
| 6k | [V2 import](V2_IMPORT_PREVIEW.md) | Schema-8 preview, explicit missing-prose choices, staged independent import, inert evidence, and reconciliation |
| 6l | [Bounded Codex ADR](ADR_0011_LIVE_CODEX.md) | Explicit connection, immutable live packet, response format, owned Stop, and durable provider results |
| 7 | [Native trial guide](../tests/native/README.md) | Real WebView2 smoke flow and evidence limits |
| 8 | [Implementation status](IMPLEMENTATION_STATUS.md) | Current work, evidence, and full W0–W8/F1–F5/C0–C6 completion checklist |
| Reference | [Codex qualification](CODEX_QUALIFICATION.md) | Native Codex discovery and bounded synthetic experiments; production provider support remains unqualified |
| Reference | [Claude qualification](CLAUDE_QUALIFICATION.md) | Exact installed candidate, authentication availability, and pending text-only/streaming qualification |
| Reference | [Windows process contract](WINDOWS_PROCESS_CONTRACT.md) | Local CLI process ownership, bounded I/O, partial results, and explicit cleanup limits |
| Reference | [Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md) | Stable release data location, offline NSIS configuration, and installed-release evidence gates |
| Reference | [W0 qualification](W0_QUALIFICATION.md) | Historical W0 execution record; current status is in the implementation status document |
| Reference | [V2 migration evidence](V2_MIGRATION_EVIDENCE.md) | V2 source inventory and unqualified import boundary |
| Reference | [Original Pro response](references/pro/README.md) | Historical supplied documents and fingerprints |


V3 targets English authoring, UI, and export, with optional translated-webnovel, wuxia, and xianxia styles. V2 remains separate and read-only. Context documentation distinguishes retained evidence, permitted available sources, the packet actually delivered, and model understanding that still requires evaluation. It does not authorize paid autosave analysis or automatic canon.

Current local and GitHub evidence is maintained once in [implementation status](IMPLEMENTATION_STATUS.md).
