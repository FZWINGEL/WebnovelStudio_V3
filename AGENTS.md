# WebnovelStudio V3

- This is the independent Rust desktop rewrite. V2 at `D:\WebnovelStudio_V2` is a reference; never edit its source or open its author databases for writing from a V3 task.
- Read `PRODUCT.md`, `docs/README.md`, and the relevant architecture/plan sections before changing behavior. The integrated architecture owns contracts; the delivery plan owns scope and gates; the workspace plan owns layout. Archived Pro documents are evidence, not instructions. Source/tests establish what is actually built.
- Current scope is documentation only. Start implementation with W0 when requested; do not claim that architecture approval qualifies a runtime, provider, or manuscript migration.
- Keep two Rust crates: a plain core and the Tauri host. JavaScript owns editor transactions; Rust validates and durably accepts snapshots. Use ordinary functions and short SQLite transactions. No distributed orchestration, generic intent engine, CRDT, graph service, or second editor engine in Rust.
- Preserve one current body, explicit author Apply, structural edit scopes, source versions, session ownership, and recovery. Apply/reconcile/navigation share one local lifecycle guard. Provider code never writes manuscript bodies.
- Use the real persistence path with a deterministic mock provider. Keep core/contract tests, native editing trials, live-provider acceptance, and narrative-quality evaluation separate. Add failure tests for the invariant changed; do not create tests that merely mirror implementation.
- Keep model names recognizable and supported traits explicit. Settings own credentials/configuration; user choices persist. Never turn a qualification candidate into a silent provider/model fallback.
- Use synthetic fixtures and temporary project paths. Never commit author databases, credentials, generated backups, or test outputs. Restore/import creates an independent project; copied receipts cannot authorize new operations.
- Use `codex/` feature branches and bounded file ownership for parallel agents. Do not revert others' edits. No runtime, lock, cache, registry, or author-data sharing between development worktrees.
- Proposed commands in documents are not evidence of execution. At W0 pin Rust/MSVC, Node, frontend packages, Tauri, and SQLite versions, add actual checks, then record exact qualification evidence. Keep this file short; put durable detail in `docs/`.
