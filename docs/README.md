# V3 documentation index

The V3 repository has a built W0 native editor baseline while W1 Rust structural scope validation and W2 core file-backed project/session/save work proceed on `codex/v3-persistence`. Documents below separate implemented behavior from later architecture and qualification gates. Source and executed checks determine status; the design documents do not turn a planned feature into a shipped one.

| Read | Document | Responsibility |
| --- | --- | --- |
| 1 | [Product requirements](../PRODUCT.md) | Author experience and explicit product boundaries |
| 2 | [Design surface](../DESIGN.md) | Built W0 writing canvas, feedback panel, and visual constraints |
| 3 | [Editor contract](ADR_0001_EDITOR_CONTRACT.md) | W0 snapshot, identity, scope, canonicalization, and replacement rules |
| 4 | [First-slice plan](V3_FIRST_SLICE_PLAN.md) | W0 status and the dependency order for W1 onward |
| 5 | [Workspace plan](V3_WORKSPACE_PLAN.md) | Repository, toolchain, data separation, and wrapper commands |
| 6 | [Refined architecture](V3_ARCHITECTURE_REFINED.md) | Later persistence, lifecycle, recovery, provider, and story contracts |
| 7 | [Native trial guide](../tests/native/README.md) | Real WebView2 smoke flow and evidence limits |
| 8 | [Implementation status](IMPLEMENTATION_STATUS.md) | Current work, evidence, and full W0–W8/F1–F5 completion checklist |
| Reference | [W0 qualification](W0_QUALIFICATION.md) | Historical W0 execution record; current status is in the implementation status document |
| Reference | [V2 migration evidence](V2_MIGRATION_EVIDENCE.md) | V2 source inventory and unqualified import boundary |
| Reference | [Original Pro response](references/pro/README.md) | Historical supplied documents and fingerprints |

V3 targets English authoring, UI, and export. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; Chinese authoring is not a product requirement. V2 remains a separate repository at `D:\WebnovelStudio_V2`, and V3 does not read or write V2 author data at runtime. Import, project persistence, providers, durable Apply, and reviewed-story behavior remain later gates.

The W0 implementation is a real Tauri/WebView2 window with a React/Tiptap editor, a Rust `validate_snapshot` command, shared JS/Rust snapshot fixtures, and session-only feedback/replacement controls. W1 and W2 are now in progress, but their work is not yet an integrated UI/persistence claim. The private GitHub repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3), with `main` as default and current main tip `d0eebfd780e435c068ef1017cac580786360d36b`. CI run [33969395869](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33969395869) passed core/frontend contract jobs, Windows workspace checks, and native Tauri build; native smoke remains open after CDP startup exceeded 20 seconds and the connection was refused before UI.
