## 1. Material write capability

- [x] 1.1 Introduce the opaque transaction- and target-bound consuming material capability with fixed adoption checkpoint origins; verify compile-fail misuse cases and transaction/checkpoint/head behavior.
- [x] 1.2 Migrate both chat and Workshop adoption call sites without changing order or transaction ownership; verify focused adoption, rollback, replay and stale/protected-source integrations.

## 2. Rust command contract

- [x] 2.1 Derive registered command names and required/optional wire arguments from parsed Rust sources using pinned Tauri semantics; verify registration, rename, injection, optionality and malformed/ambiguous syntax fixtures.
- [x] 2.2 Generate the source-hashed command manifest through the bindings tool and add independent drift enforcement; verify stale-source/registration detection and unchanged nine wire-type modules.

## 3. Frontend parity enforcement

- [x] 3.1 Replace the naming regex with parsed invoke binding/call extraction and reject unresolved bypasses; verify aliases, namespaces, shadowing, comments and dynamic-syntax fixtures.
- [x] 3.2 Join all real calls to the current source-hashed Rust contract, validating registered names and required/optional/extra keys; verify positive/negative fixtures, real inventory and pinned TypeScript.

## 4. Integrated qualification

- [x] 4.1 Independently review and repair the combined capability and parity changes; verify identified regressions are covered and prior local work is preserved.
- [x] 4.2 Run the pinned full check after final repairs; verify formatting, strict Clippy, Rust tests, TypeScript, build, frontend and tooling checks succeed with recorded counts.
- [x] 4.3 Build a fresh native app and run Workshop and chat flows sequentially; verify both pass on the recorded executable hash with synthetic-only qualification limits.
- [x] 4.4 Update the current architecture map, implementation status and evidence report, reconcile all tasks, and validate OpenSpec; verify all ten tasks are complete with no deferred implementation.
