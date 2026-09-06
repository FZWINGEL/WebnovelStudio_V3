# WebnovelStudio V3

- V3 is the independent Rust/Tauri rewrite. V2 at `D:\WebnovelStudio_V2` is read-only reference material for V3 work; do not edit V2 or its author data.
- Read [PRODUCT.md](PRODUCT.md), [docs/README.md](docs/README.md), and the relevant architecture/plan before changing behavior. Source and executed checks establish current status; planned checkboxes do not.
- The user has authorized completing V3 beyond W0. Follow the dependency order in the first-slice plan; W1/W2 persistence and reconciliation precede a persistent writing UI. The W0 trial remains session-only until that path is integrated and verified. Keep current evidence and remaining work in `docs/IMPLEMENTATION_STATUS.md`.
- The product targets English authoring, UI, and export. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; do not add Chinese authoring or IME acceptance requirements.
- Codex is the primary provider focus. Keep the Claude and OpenAI-compatible integrations; further adapter ports are deferred at the author's request. Summary and story-memory model calls use GPT-5.6 Luna/xhigh independently of the writing-model picker.
- JavaScript owns live Tiptap/ProseMirror transactions. Rust independently validates the restricted snapshot contract over IPC. Do not add a general ProseMirror-step interpreter or a second editor engine.
- Preserve the later lifecycle and restore decisions: explicit project/document/session identity, conservative stale-edit refusal, short local Apply barriers, and restore into a new recovered project. These become durable implementation work after W0.
- Keep toolchain and package pins in the root manifests. Use synthetic fixtures and temporary paths; never commit author databases, credentials, generated native results, or backups.
- Proposed commands are not evidence. Record native WebView2 text input, accessibility, persistence, provider, and packaging evidence in the qualification documents before claiming support.
- Register new core integration test files in `crates/core/tests/integration.rs`; see [development checks](docs/TESTING.md) for focused commands and the full verification path.
