# V3 documentation index

The V3 repository has a built W0 native editor baseline and a persistent Library/Workspace surface. W1 Rust structural scope validation is implemented with fixtures, W2 core and frontend session work is implemented, and W3 registry/transfer work is active; broader author-trial integration and qualification remain open. Documents below separate implemented behavior from later architecture and qualification gates. Source and executed checks determine status; the design documents do not turn a planned feature into a shipped one.

| Read | Document | Responsibility |
| --- | --- | --- |
| 1 | [Product requirements](../PRODUCT.md) | Author experience and explicit product boundaries |
| 2 | [Design surface](../DESIGN.md) | Built W0 writing canvas, feedback panel, and visual constraints |
| 3 | [Editor contract](ADR_0001_EDITOR_CONTRACT.md) | W0 snapshot, identity, scope, canonicalization, and replacement rules |
| 4 | [First-slice plan](V3_FIRST_SLICE_PLAN.md) | W0 status and the dependency order for W1 onward |
| 5 | [Workspace plan](V3_WORKSPACE_PLAN.md) | Repository, toolchain, data separation, and wrapper commands |
| 6 | [Refined architecture](V3_ARCHITECTURE_REFINED.md) | Later persistence, lifecycle, recovery, provider, and story contracts |
| 6a | [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) · [first slice](V3_STORY_CONTEXT_FIRST_SLICE.md) | Adopted context extension; C0–C6 remain planned and preserve save/Apply/authority ownership |
| 7 | [Native trial guide](../tests/native/README.md) | Real WebView2 smoke flow and evidence limits |
| 8 | [Implementation status](IMPLEMENTATION_STATUS.md) | Current work, evidence, and full W0–W8/F1–F5/C0–C6 completion checklist |
| Reference | [W0 qualification](W0_QUALIFICATION.md) | Historical W0 execution record; current status is in the implementation status document |
| Reference | [V2 migration evidence](V2_MIGRATION_EVIDENCE.md) | V2 source inventory and unqualified import boundary |
| Reference | [Original Pro response](references/pro/README.md) | Historical supplied documents and fingerprints |

V3 targets English authoring, UI, and export. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; Chinese authoring is not a product requirement. V2 remains a separate repository at `D:\WebnovelStudio_V2`, and V3 does not read or write V2 author data at runtime. Broader author-trial integration and full persistence/reconciliation qualification, import, providers, durable Apply, and reviewed-story behavior remain later gates. Context documentation distinguishes stored evidence, permitted available sources, the packet actually delivered, and model-understood behavior that still requires evaluation; it does not authorize paid autosave analysis, automatic canon, or replacement of source text with a huge summary.

The default native UI now provides a persistent Library/Workspace with blank projects, optional document types, Rust SQLite autosave, flush-before-switch, rename, duplicate, archive/unarchive, and native folder/backup/recovery/TXT dialogs. W1 structural scope validation and shared JS/Rust fixtures are implemented; W2 session/core receipts and reconciliation are implemented; W3 registry/transfer work is active. The explicit W0 sample editor remains session-only, and persistent UI qualification is still open. The private GitHub repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3), with `main` as default and current main tip `d0eebfd780e435c068ef1017cac580786360d36b`. The latest CI run [33971040177](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33971040177), for commit `b8d4cb8`, reached the UI but failed native smoke while waiting for a CSS-hidden diagnostic label; the wait/query fix is local and the remote rerun is pending. No rerun result is claimed here.

Current local checks include the root wrapper's formatting, workspace Clippy, Rust tests, TypeScript/Vite build, and frontend tests. The rebuilt real Tauri/WebView2 smoke path passed 14/14 checks; native backup/export dialog use and the broader A, N, and W3 qualification remain open. See [implementation status](IMPLEMENTATION_STATUS.md) for the compact evidence record.
