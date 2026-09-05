# V3 workspace and implementation handoff

**Decision date:** 5 September 2026. **User choice:** a separate V3 repository alongside V2. **Current execution:** W0 native editor baseline retained while W1 Rust structural scope validation and W2 core file-backed project/session/save work proceed on `codex/v3-persistence`.

**Language scope:** English authoring, UI, and export. Wuxia, xianxia, cultivation, and translated-Chinese-webnovel register/terminology are optional English writing styles. Chinese-language authoring and Pinyin qualification are not product requirements. Unicode regression fixtures remain internal correctness checks.

W0's runtime UI remains session-only. W1 Rust structural scope validation and W2 core file-backed projects, sessions, and saves are in progress; they do not yet establish integrated UI/persistence behavior or complete the V3 goal.

## 1. Repository boundary

| Location | Role | Write boundary |
| --- | --- | --- |
| `D:\WebnovelStudio_V2` | Validated Node/Express V2 reference and its existing local state | V3 tasks read source only. V2 maintenance remains a separate task. |
| `D:\WebnovelStudio_V3` | Independent Git repository for the Rust desktop application | V3 architecture, source, tests, lockfiles, and build configuration live here. |
| `D:\WebnovelStudio_V3\.worktrees\<task>` | Optional later isolated V3 implementation checkouts | One `codex/` feature branch per task; ignored by the enclosing checkout. |
| Author-selected project directory | Manuscript SQLite database, marker, assets, and history | Opened only by an explicitly selected V3 project session; never committed as source. |

V2's reference commit is `c41c6e45c41cfdbcfcf51aa4840605efb4975845`, with [Windows and Ubuntu CI success](https://github.com/FZWINGEL/WebnovelStudio_V2/actions/runs/33963410139). That is V2 evidence only. V3 does not inherit its test results or user data.

A V2 worktree would still belong to V2's repository and share its Git history and refs. A sibling repository gives V3 its own root instructions, Cargo/npm dependencies, CI, releases, and application identity. This is a local architectural decision, informed by [Git's worktree model](https://git-scm.com/docs/git-worktree); it does not imply cross-repository atomic operations.

Do not relocate V2, replace its `src/`, add Rust to its package scripts, or copy its `.git`, `node_modules`, caches, databases, local credentials, or old Spec Kit implementation checkboxes into V3. Migration consumes a consistent, explicitly selected snapshot through the importer. No application component should depend at runtime on `../WebnovelStudio_V2`.

The foundation branch is `codex/v3-foundation`; the W0 implementation branch was `codex/v3-native-editor-spike`; current implementation is on `codex/v3-persistence`. The private GitHub repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3), with `main` as its default branch and current main tip `d0eebfd780e435c068ef1017cac580786360d36b`. V2 retains its own remote and release history.

## 2. Planned source layout

This is the full planned layout. W0 created the two Cargo members, one frontend package, restricted editor/IPC/shell modules, shared fixtures, and executable checks. W1/W2 now add core structural validation and file-backed project/session/save work; later UI integration and provider/storage modules remain planned:

```text
WebnovelStudio_V3/
  AGENTS.md, PRODUCT.md, README.md
  Cargo.toml                   # virtual workspace, two members
  Cargo.lock                   # committed for this application
  rust-toolchain.toml          # exact qualified Rust + MSVC target
  .node-version                # exact qualified Node LTS
  apps/desktop/
    package.json, package-lock.json
    vite.config.ts, tsconfig.json
    src/
      shell/                   # library, notes/manuscript navigation
      editor/                  # schema, identity, anchors, session, history
      assistant/               # discussion, scope, proposal previews
      ipc/                     # generated/checked DTOs and thin calls
    src-tauri/
      Cargo.toml, tauri.conf.json
      capabilities/
      src/                     # composition, commands, menus, dialogs
  crates/core/
    Cargo.toml
    src/{projects,documents,feedback,storage,transfer}/
    src/{context,jobs}.rs
    src/providers/             # mock first; qualified adapter later
    tests/                     # file-backed integration/failure fixtures
  contracts/fixtures/          # shared canonical documents and expected results
  tests/native/                # actual desktop flows and manual trial records
  scripts/                     # small build/qualification helpers when needed
  docs/
```

The two Cargo members are `crates/core` and `apps/desktop/src-tauri`. The desktop host depends on the core; the core never depends on Tauri or React. Keep domain modules ordinary until a real independent package requires extraction. Cargo workspaces provide the shared lockfile and build target structure; choose explicit workspace membership and edition/resolver values when locking the toolchain. [Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html).

Use one frontend npm package and one committed `apps/desktop/package-lock.json`. No Nx/Turborepo or second frontend is needed. React components do not issue SQL or spawn providers. Node/Vite are development/build tools; the packaged author application uses local web assets in its own Tauri window, with no Node application server or browser startup step.

Contracts use a small explicit document schema and typed DTOs. Generate types from one chosen source or validate shared fixtures in both languages; do not maintain two independently evolving protocol definitions. Canonicalization, scope tokens, IDs, error variants, receipt namespace, and lifecycle transitions must be locked before persistence/Apply work expands.

## 3. Data and development isolation

| Data | Planned location/policy |
| --- | --- |
| Production app registry and nonsecret settings | `%LOCALAPPDATA%\WebnovelStudioV3` using the selected application identifier |
| Suggested new-project destination | `%USERPROFILE%\WebnovelStudio\Projects`; author can choose another local folder |
| Manual backup destination | Author-selected native destination; explain that a copy on the same drive is not drive-loss protection |
| Development app registry and projects | `%LOCALAPPDATA%\WebnovelStudioV3-Dev\<checkout-id>`; a different namespace for every development worktree |
| Automated test projects | Unique temporary directories per test, never the production registry or author folders |
| Credentials | OS credential store or the provider's managed login; core-owned references, no project/export secrets |

These are planned defaults; the design bootstrap created no author-data folders. Development builds must show that they use development data. Their New Project destination is inside their development root, not the production default. Test-only path overrides and fault controls must be excluded from shipping builds. A separate app identifier prevents production and development single-instance activation from redirecting into each other.

Every project session owns a resolved path, project ID, current operation namespace, connection, and OS-held lock. Runtime code never resolves a write through a mutable global "current project" path. Project-folder identity checks cover aliases/junctions and duplicate embedded IDs. Same-project stale callbacks are fenced; cross-project callbacks are routed by their captured ownership.

Do not share a runtime registry, SQLite writer, build `target` directory override, or author fixture between parallel worktrees. Shared dependency download caches managed by Cargo/npm are fine; application state is not. Never copy a live SQLite main file to seed a test or import. A synthetic fixture builder creates new databases; an importer reads a consistent backup as described in [migration evidence](V2_MIGRATION_EVIDENCE.md).

## 4. Host readiness observed during design

The following is the historical read-only inventory taken before W0. Rust 1.98.1, Node 24.20.0, and the locked application dependencies are now installed and Windows builds pass; [W0 qualification](W0_QUALIFICATION.md) records current versions and executed results. Preserve this table as the pre-implementation observation:

| Component | Observed state | W0 action |
| --- | --- | --- |
| Windows | `Win32_OperatingSystem`: Windows 11 Pro, 64-bit, build `10.0.26200` | Record the actual runtime and input methods used in the native trial. |
| Rust/Cargo | Neither command found on PATH nor in the default `%USERPROFILE%\.cargo\bin` location | Install/locate a supported Rust toolchain when implementing W0; select `x86_64-pc-windows-msvc`, then pin the tested release. This was not a whole-disk inventory. |
| C++ build tools | `vswhere` found Visual Studio Build Tools 2026, version `18.4.11626.88`, with the x86/x64 C++ tools component | Verify MSVC linking and an applicable Windows SDK with a real Tauri build. Presence alone is insufficient. |
| WebView2 | EdgeUpdate registry reports WebView2 Runtime `152.0.4191.62` | Query the runtime from the actual Tauri process and exercise it; registry presence is not native-editor qualification. |
| Node/npm | Global Node `25.9.0`, npm `11.12.1`; V2 validated Node `24.20.0` separately | Start the V3 lock experiment with Node `24.20.0` LTS and record the npm version used. Do not change V2's runtime installation. |
| V3 source | Documentation repository only | Create the minimum two-crate/native-editor scaffold at W0. |

Tauri's Windows prerequisites include C++ build tools, WebView2, and Rust; follow its [official prerequisites](https://v2.tauri.app/start/prerequisites/) when performing W0. No installer, toolchain download, credential entry, or provider request was run during the historical design integration. W0 subsequently installed the official Rust toolchain and built the native application; no provider or author-data operation was added.

## 5. First implementation task: W0 only

The bounded `codex/v3-native-editor-spike` implementation now exists in the V3 workspace. Its outcome is a small actual Tauri window and recorded experiments, not the whole application or all of milestone A.

1. Verify/install the missing development prerequisites; pin Rust, Node, Tauri, React/Tiptap/PM, and SQLite dependencies with lockfiles. Record native runtime versions separately from package versions.
2. Scaffold the two-crate workspace and one frontend package. The editor uses the restricted document schema; a thin Rust command round-trips and validates a synthetic snapshot.
3. Exercise English keyboard/dead-key input, accented or transliterated names and emoji, formatted and repeated selections, composer focus transfer, scene breaks, paste, and a strict local replacement/undo cycle in the real window.
4. Record the representation/identity/scope decisions and shared fixtures. A provisional hash implementation becomes contractual only when JS/Rust fixtures agree. Do not claim proposal Apply durability from this editor spike.
5. Record failures and precise reproduction steps. Decide whether the default Tauri/Tiptap combination remains suitable before expanding W1/W2. No automatic migration or live provider is needed.

At W0 define actual commands corresponding to `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, frontend type/contract tests, and Tauri dev/build. The frontend can expose `desktop:dev` and `desktop:build` wrappers around Tauri. These commands are now implemented and exercised. The root `scripts/desktop.ps1` wrapper selects Node 24.20.0 through npm exec and adds the user Cargo bin directory; use `setup`, `dev`, `spike`, `build`, `check`, `native`, or `test`. Close the trial executable before rebuilding it on Windows; `native` requires a successful `spike` build. Exact evidence remains in the qualification record.

Keep the first editor instance mounted across unrelated shell/chat updates. After W0, W1/W2 establish schema, persistence and save reconciliation; W3 enables the A writing trial. The [delivery plan](V3_FIRST_SLICE_PLAN.md) determines all later gates. Do not turn this first task into W0–W8.

## 6. CI, review, and acceptance

V3 has its own `.github/workflows/ci.yml` and the private GitHub repository uses `main` as its default branch. CI run [33969395869](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33969395869) passed Ubuntu and Windows core/frontend contract jobs, Windows workspace Clippy/tests, and the native Tauri build. Native smoke failed before the UI because CDP startup exceeded 20 seconds and the connection was refused; native smoke remains open. Browser frontend checks do not certify native WebView2 keyboard or accessibility behavior. Live-provider trials use explicit opt-in and credentials outside normal CI; narrative evaluation remains a separate result.

Document-only checks validate links, source hashes, status/scope consistency, and whitespace. Do not add empty passing runtime jobs to imply an application exists. Every later failure gate becomes executable with its owning feature; absent deferred features have no visible action until implemented and tested.

Keep one owner for the document/session protocol while parallel work covers independent UI, source fixtures, and adapters. Each implementation change states the affected invariant, an acceptance example, and the appropriate tests. Save/Apply/recovery semantics require failure tests; simple reversible UI copy changes do not need an invented test framework.

The operational design has supported the W0 implementation. Native-editor behavior, exact dependency compatibility, live providers, narrative quality, and actual-manuscript migration remain experiments with explicit gates, rather than unresolved reasons to write another general architecture proposal.
