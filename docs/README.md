# V3 documentation index

The V3 repository is now on the W0 native editor spike. Documents below separate implemented behavior from later architecture and qualification gates. Source and executed native checks determine status; the design documents do not turn a planned feature into a shipped one.

| Read | Document | Responsibility |
| --- | --- | --- |
| 1 | [Product requirements](../PRODUCT.md) | Author experience and explicit product boundaries |
| 2 | [Design surface](../DESIGN.md) | Built W0 writing canvas, feedback panel, and visual constraints |
| 3 | [Editor contract](ADR_0001_EDITOR_CONTRACT.md) | W0 snapshot, identity, scope, canonicalization, and replacement rules |
| 4 | [First-slice plan](V3_FIRST_SLICE_PLAN.md) | W0 status and the dependency order for W1 onward |
| 5 | [Workspace plan](V3_WORKSPACE_PLAN.md) | Repository, toolchain, data separation, and wrapper commands |
| 6 | [Refined architecture](V3_ARCHITECTURE_REFINED.md) | Later persistence, lifecycle, recovery, provider, and story contracts |
| 7 | [Native trial guide](../tests/native/README.md) | Real WebView2 smoke flow and evidence limits |
| 8 | [W0 qualification](W0_QUALIFICATION.md) | Execution record and current qualification verdict |
| Reference | [V2 migration evidence](V2_MIGRATION_EVIDENCE.md) | V2 source inventory and unqualified import boundary |
| Reference | [Original Pro response](references/pro/README.md) | Historical supplied documents and fingerprints |

V3 targets English authoring, UI, and export. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; Chinese authoring is not a product requirement. V2 remains a separate repository at `D:\WebnovelStudio_V2`, and V3 does not read or write V2 author data at runtime. Import, project persistence, providers, durable Apply, and reviewed-story behavior remain later gates.

The W0 implementation is a real Tauri/WebView2 window with a React/Tiptap editor, a Rust `validate_snapshot` command, shared JS/Rust snapshot fixtures, and session-only feedback/replacement controls. It does not yet provide author storage or a native release. Internal Unicode fixtures cover English names, accents, and emoji. The remaining English native author trial, minimum-window behavior, external Word paste, screen-reader use, and broader native qualification remain open.
