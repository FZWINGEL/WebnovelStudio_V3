# V3 workspace and implementation handoff

**Decision date:** 5 September 2026. **User choice:** a separate V3 repository alongside V2. **Current execution:** documentation-only repository initialized; application and toolchain setup remain W0 work.

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

The local foundation branch is `codex/v3-foundation`. No GitHub repository or remote was created during design work. When publishing is requested, create the V3 remote explicitly, choose its default branch, push the reviewed foundation, and verify its CI independently. Retain the V2 remote and release history.

## 2. Planned source layout

This is the layout to scaffold at W0, not a claim that these code files exist:

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

The following is a read-only inventory from 5 September 2026, not a successful native build:

| Component | Observed state | W0 action |
| --- | --- | --- |
| Windows | `Win32_OperatingSystem`: Windows 11 Pro, 64-bit, build `10.0.26200` | Record the actual runtime and input methods used in the native trial. |
| Rust/Cargo | Neither command found on PATH nor in the default `%USERPROFILE%\.cargo\bin` location | Install/locate a supported Rust toolchain when implementing W0; select `x86_64-pc-windows-msvc`, then pin the tested release. This was not a whole-disk inventory. |
| C++ build tools | `vswhere` found Visual Studio Build Tools 2026, version `18.4.11626.88`, with the x86/x64 C++ tools component | Verify MSVC linking and an applicable Windows SDK with a real Tauri build. Presence alone is insufficient. |
| WebView2 | EdgeUpdate registry reports WebView2 Runtime `152.0.4191.62` | Query the runtime from the actual Tauri process and exercise it; registry presence is not native-editor qualification. |
| Node/npm | Global Node `25.9.0`, npm `11.12.1`; V2 validated Node `24.20.0` separately | Start the V3 lock experiment with Node `24.20.0` LTS and record the npm version used. Do not change V2's runtime installation. |
| V3 source | Documentation repository only | Create the minimum two-crate/native-editor scaffold at W0. |

Tauri's Windows prerequisites include C++ build tools, WebView2, and Rust; follow its [official prerequisites](https://v2.tauri.app/start/prerequisites/) when performing W0. No installer, toolchain download, credential entry, or provider request was run during this design integration.

## 5. First implementation task: W0 only

Create a bounded `codex/v3-native-editor-spike` implementation task in the V3 workspace when implementation starts. Its outcome is a small actual Tauri window and recorded experiments, not the whole application or all of milestone A.

1. Verify/install the missing development prerequisites; pin Rust, Node, Tauri, React/Tiptap/PM, and SQLite dependencies with lockfiles. Record native runtime versions separately from package versions.
2. Scaffold the two-crate workspace and one frontend package. The editor uses the restricted document schema; a thin Rust command round-trips and validates a synthetic snapshot.
3. Exercise Microsoft Pinyin composition, mixed Chinese/English/emoji, formatted and repeated selections, composer focus transfer, scene breaks, paste, and a strict local replacement/undo cycle in the real window.
4. Record the representation/identity/scope decisions and shared fixtures. A provisional hash implementation becomes contractual only when JS/Rust fixtures agree. Do not claim proposal Apply durability from this editor spike.
5. Record failures and precise reproduction steps. Decide whether the default Tauri/Tiptap combination remains suitable before expanding W1/W2. No automatic migration or live provider is needed.

At W0 define actual commands corresponding to `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, frontend type/contract tests, and Tauri dev/build. The frontend can expose `desktop:dev` and `desktop:build` wrappers around Tauri. These are intended command contracts; they are not runnable or verified in this documentation-only repository.

Keep the first editor instance mounted across unrelated shell/chat updates. After W0, W1/W2 establish schema, persistence and save reconciliation; W3 enables the A writing trial. The [delivery plan](V3_FIRST_SLICE_PLAN.md) determines all later gates. Do not turn this first task into W0–W8.

## 6. CI, review, and acceptance

V3 gets its own workflow once executable packages exist. Core and shared-contract tests can run on Windows/Linux; actual Windows MSVC/Tauri builds and native journeys are a separate lane. Browser frontend checks do not certify WebView2 IME or accessibility. Live-provider trials use explicit opt-in and credentials outside normal CI; narrative evaluation remains a separate result.

Document-only checks validate links, source hashes, status/scope consistency, and whitespace. Do not add empty passing runtime jobs to imply an application exists. Every later failure gate becomes executable with its owning feature; absent deferred features have no visible action until implemented and tested.

Keep one owner for the document/session protocol while parallel work covers independent UI, source fixtures, and adapters. Each implementation change states the affected invariant, an acceptance example, and the appropriate tests. Save/Apply/recovery semantics require failure tests; simple reversible UI copy changes do not need an invented test framework.

The operational design is complete enough to start W0. Native-editor behavior, exact dependency compatibility, live providers, narrative quality, and actual-manuscript migration remain experiments with explicit gates, rather than unresolved reasons to write another general architecture proposal.
