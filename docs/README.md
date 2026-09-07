# V3 documentation index

This index is for the private WebnovelStudio V3.0.0 Windows development candidate, an English web-novel workspace with optional wuxia, xianxia, cultivation, progression, and translated-register styles. Start with the [root README](../README.md) for setup and the [3.0.0 release preparation](RELEASE_3_0_0.md) plus [changelog](../CHANGELOG.md) for candidate scope and user-facing changes.

Use [implementation status](IMPLEMENTATION_STATUS.md) for current tests, native and provider evidence, and remaining qualification. Codex is the primary provider; summaries and Story Memory use GPT-5.6 Luna/xhigh. Claude and OpenAI-compatible endpoints remain available, with further adapter ports deferred. Source and executed checks establish current behavior; the design documents below define contracts and target architecture.

| Read | Document | Responsibility |
| --- | --- | --- |
| 0 | [Release preparation](RELEASE_3_0_0.md) | Candidate identity, verification, and remaining package/native gates |
| 0a | [Changelog](../CHANGELOG.md) | User-facing changes for the current unreleased candidate |
| 0b | [Testing and CI audit](V3_TESTING_CI_PERFORMANCE_AUDIT.md) | Fetched read-only performance audit of `c83a127`; proposed follow-up work and measured evidence, separate from Workshop implementation |
| 1 | [Product requirements](../PRODUCT.md) | Author experience and explicit product boundaries |
| 2 | [Design surface](../DESIGN.md) | Current AI writing workspace, project tabs, assistant, and historical editor trial |
| 2a | [Story Workshop specification](V3_STORY_WORKSHOP_UX_SPEC.md) · [implementation ledger](V3_STORY_WORKSHOP_IMPLEMENTATION.md) · [author study](STORY_WORKSHOP_AUTHOR_STUDY.md) | Develop/Write, contrasting candidates, scoped preferences, decisions and relationships; implementation and qualification tracked separately |
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
| 6m | [Author review ADR](ADR_0012_AUTHOR_REVIEW.md) | Development slice for exact staged chapter/revision review, explicit Mark, restart/changed-earlier status, and recovered-copy boundaries; full F2 remains open |
| 6n | [Reviewed context ADR](ADR_0013_REVIEWED_CONTEXT.md) | Exact earlier authority manifests, working continuation target, historical evidence, and the separate dispatch boundary |
| 6o | [Chapter memory ADR](ADR_0014_CHAPTER_MEMORY.md) | C4 implementation contract for explicit chapter analysis, exact evidence, generated views, and local result recovery |
| 6p | [Navigation context ADR](ADR_0015_NAVIGATION_CONTEXT.md) | C4-B frozen generated-view identities, automatic discussion packing, exact evidence, historical retention, and coverage inspection |
| 6q | [Story continuation contract](ADR_0016_STORY_CONTINUATION.md) | Local slice plus one bounded live continuation result: Working/Reviewed generation, typed append preview, and atomic Apply/Reject; CI-qualified development slice; broader provider/release qualification pending |
| 6r | [Reviewed export ADR](ADR_0017_REVIEWED_EXPORT.md) | Schema-20 reviewed export CI-qualified as a development slice: exact author-reviewed chapter export, freshness acceptance, and historical file receipts; local wrapper/native checks pass, strict CI 34010306332 passes all 38 native checks |
| 6s | [Reviewed story evidence ADR](ADR_0018_REVIEWED_STORY_EVIDENCE.md) | Schema-21 CI-qualified development slice: exact passage-backed possession records, immutable review sets, audience-filtered context delivery, and distinct evidence inspection; CI 34012813796 passes all 40 strict native checks |
| 6t | [Evidence history ADR](ADR_0019_EVIDENCE_HISTORY.md) | C5-A development slice, CI-qualified with all 41 native checks: project entity reuse, batched current-evidence freeze, authenticated object history, disclosure filtering, and incomplete observations |
| 6v | [Promise history ADR](ADR_0021_PROMISE_HISTORY.md) | Schema-23 C5 development slice: explicit promise observations, cross-chapter identity reuse, authenticated history, reader projection, and incomplete evidence; current qualification evidence is maintained in implementation status, with broader live and author-trial gates still open |
| 6u | [Structured suggestions ADR](ADR_0020_STRUCTURED_SUGGESTIONS.md) | Local development slice for explicit block-range and whole-chapter suggestions: typed rich blocks, editor-owned IDs, protected surroundings, and atomic Apply; local native 42/43 with the known clipboard case omitted; CI 34017484597 passes all 43 strict checks |
| 6w | [Bounded story lookups ADR](ADR_0022_BOUNDED_STORY_LOOKUPS.md) | Original C6 development contract, extended by ADR 0032: opt-in `story-lookup.v1` search/read route for Working, AuthorRoom, and Discuss; schema-24 durable invocation/read records under the current schema-34 project reader floor, optional exact frozen source-title projection for returned child sources, an initial call plus at most two further calls, and per-packet inspection; checkpoint CI passes all 47 strict native checks and the HTTP fixture, while live-provider and release qualification remain pending |
| 6x | [OpenAI-compatible endpoints ADR](ADR_0023_OPENAI_COMPATIBLE.md) | Configurable endpoint profiles, native credential readiness, model catalog/manual entries, V2-style provider selection rails, and the bounded native HTTP transport; HTTP context lookup, V2 CLI adapter parity, and native/live qualification remain open |
| 6y | [Dynamic Codex models ADR](ADR_0024_DYNAMIC_CODEX_MODELS.md) | Explicit Settings or narrowly guarded startup read-only interactive `model/list` discovery, sanitized display-only catalog, exact author selection binding, frozen CLI identity and descriptor fingerprint, and current schema-28 reader-floor compatibility; wrapper and bounded Luna/Mini qualification pass while wider provider/live/release qualification remain open |
| 6z | [Story Memory provider ADR](ADR_0025_API_STORY_MEMORY.md) | Independent CAS-backed Story Memory choice for fixed Codex, explicit mock, or configured HTTP; HTTP route/configuration and private credential are captured at acceptance, schema-29 memory delivery receipts are retained, and local recovery never replays a POST; accepted native synthetic evidence exists while hosted/live and release qualification remain open |
| 6ab | [AI writing workspace ADR](ADR_0027_AI_WRITING_WORKSPACE.md) | User-directed project tabs, shared chapter navigation, explicit Draft/Continue/Develop and Send/Apply review flow, guarded read-only startup Codex check, and scope-gated nonchapter development; schema-31 migration, local contracts, native regression, and one live Codex draft verified; broader qualification remains open |
| 6ac | [Recovery copy ADR](ADR_0028_RECOVERY_COPY.md) | Click-time immutable editor-buffer copy to a new Markdown file after save failure; no project/database dependency, overwrite refusal, receipt verification, or automatic retry; native/package qualification remains pending |
| 6ad | [Normal close ADR](ADR_0029_NORMAL_CLOSE.md) | Shared editor flush, native generation admission, all-project Stop and cleanup, retained-result blocking, exact orphan interruption, and Stay open; current executed qualification is recorded in implementation status |
| 6ae | [Accepted summaries ADR](ADR_0030_ACCEPTED_SUMMARIES.md) | Optional immutable chapter summaries in author review, explicit generated-memory starting points, source/basis fences, audience filtering, and distinct context delivery receipts |
| 6af | [Character knowledge ADR](ADR_0031_CHARACTER_KNOWLEDGE.md) | Passage-backed character attitudes, stable character/topic identities, immutable review sets, reader-filtered context and incomplete knowledge history; qualification is recorded in implementation status |
| 6ag | [Reviewed-memory lookup ADR](ADR_0032_STORY_MEMORY_LOOKUPS.md) | Frozen identity catalogs and paged knowledge, promise and possession history in the existing bounded read loop; schema-34 reader boundary, legacy capability preservation and per-invocation inspection |
| 7 | [Native trial guide](../tests/native/README.md) | Real WebView2 smoke flow and evidence limits |
| 8 | [Implementation status](IMPLEMENTATION_STATUS.md) | Current work, evidence, and full W0–W8/F1–F5/C0–C6 completion checklist |
| Reference | [Codex qualification](CODEX_QUALIFICATION.md) | Native Codex discovery and bounded synthetic experiments; production provider support remains unqualified |
| 6aa | [Claude author ADR](ADR_0026_CLAUDE_AUTHOR.md) | Integrated native author development surface: static three-model picker, five effort choices, frozen `claude-stdin.author.v1` binding, schema-30 reported-model evidence, and no memory/lookup route; native picker and synthetic transport checks pass while live Claude qualification remains open |
| Reference | [Claude qualification](CLAUDE_QUALIFICATION.md) | Installed/authentication evidence, synthetic foundation history, current author-slice boundaries, and remaining native/live gates |
| Reference | [Windows process contract](WINDOWS_PROCESS_CONTRACT.md) | Local CLI process ownership, bounded I/O, partial results, and explicit cleanup limits |
| Reference | [Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md) | Stable release data location, offline NSIS configuration, and installed-release evidence gates |
| Reference | [W0 qualification](W0_QUALIFICATION.md) | Historical W0 execution record; current status is in the implementation status document |
| Reference | [V2 migration evidence](V2_MIGRATION_EVIDENCE.md) | V2 source inventory and unqualified import boundary |
| Reference | [Original Pro response](references/pro/README.md) | Historical supplied documents and fingerprints |


V3 targets English authoring, UI, and export, with optional translated-webnovel, wuxia, and xianxia styles. V2 remains separate and read-only. Context documentation distinguishes retained evidence, permitted available sources, the packet actually delivered, and model understanding that still requires evaluation. It does not authorize paid autosave analysis or automatic canon.

Use [development checks](TESTING.md) for focused commands, full verification, and integration-suite registration.
