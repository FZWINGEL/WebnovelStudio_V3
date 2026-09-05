# V3 design baseline

**Integrated 5 September 2026.** Read these documents as the design for the next implementation, with runtime and qualification work still pending.

| Read | Document | Responsibility |
| --- | --- | --- |
| 1 | [Product requirements](../PRODUCT.md) | Confirmed author experience and user choices |
| 2 | [Delivery plan](V3_FIRST_SLICE_PLAN.md) | Native spike, Writing, Feedback, Release qualification; scoped completion criteria |
| 3 | [Workspace plan](V3_WORKSPACE_PLAN.md) | Separate repository, layout, toolchain, development data, and first task |
| 4 | [Refined architecture](V3_ARCHITECTURE_REFINED.md) | Document/scope, save/apply/reconciliation, history, recovery, provider, and later story contracts |
| 5 | [V2 migration evidence](V2_MIGRATION_EVIDENCE.md) | Source-backed inventory and unqualified import boundaries |
| Reference | [Original Pro response](references/pro/README.md) | Unmodified supplied documents, fingerprints, and inspection attribution |

The integrated architecture specifies behavior. The plan selects when it is built, and the workspace plan selects where it lives. Later architecture sections are not automatically first-milestone requirements. `AGENTS.md` supplies concise engineering rules. Source and executed tests determine implementation status; the originals and earlier proposals never override explicit user requirements.

The integration resolves three review findings: a shared Apply/reconcile/navigation lifecycle guard, an explicit namespace for copied command receipts, and same-document revision parentage. It also makes ordinary edit requests direct, keeps Import V2 out of the initial usable surface, and separates early author trials from release qualification.

The [original V3 proposal and research](https://github.com/FZWINGEL/WebnovelStudio_V2/tree/c41c6e45c41cfdbcfcf51aa4840605efb4975845/docs/v3) remain historical inputs in V2. Its [root proposal](https://github.com/FZWINGEL/WebnovelStudio_V2/blob/c41c6e45c41cfdbcfcf51aa4840605efb4975845/V3_RUST_REWRITE_PROPOSAL.md) and [NewResearch](https://github.com/FZWINGEL/WebnovelStudio_V2/tree/c41c6e45c41cfdbcfcf51aa4840605efb4975845/NewResearch) can inform later decisions without becoming another competing contract.

No V3 test, installed application, live-provider integration, or actual-manuscript import has been qualified by this document integration.

## Integration validation

The integration checked local Markdown link targets, verified both original Pro files against their SHA-256 fingerprints, and executed the architecture's SQL sketch in an isolated in-memory SQLite database. The sketch accepted same-document revision parents, rejected cross-document parents, and kept identical operation IDs distinct across receipt namespaces. These checks validate documentation/examples; they do not test a Rust implementation or establish runtime recovery.

Independent document reviews checked contract consistency, delivery dependencies, deferred feature gates, and V2/V3 workspace separation. Remaining experiments are explicitly assigned to W0 and the later acceptance gates.
