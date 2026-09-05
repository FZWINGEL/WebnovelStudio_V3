# WebnovelStudio V3

- V3 is the independent Rust/Tauri rewrite. V2 at `D:\WebnovelStudio_V2` is read-only reference material for V3 work; do not edit V2 or its author data.
- Read [PRODUCT.md](PRODUCT.md), [docs/README.md](docs/README.md), and the relevant architecture/plan before changing behavior. Source and executed checks establish current status; planned checkboxes do not.
- W0 is in progress on `codex/v3-native-editor-spike`. Keep the spike bounded: sample text is session-only, Rust validates snapshots, and no author storage, provider, SQLite persistence, durable Apply, receipts, or reconciliation are present yet.
- The product targets English authoring, UI, and export. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; do not add Chinese authoring or IME acceptance requirements.
- JavaScript owns live Tiptap/ProseMirror transactions. Rust independently validates the restricted snapshot contract over IPC. Do not add a general ProseMirror-step interpreter or a second editor engine.
- Preserve the later lifecycle and restore decisions: explicit project/document/session identity, conservative stale-edit refusal, short local Apply barriers, and restore into a new recovered project. These become durable implementation work after W0.
- Keep toolchain and package pins in the root manifests. Use synthetic fixtures and temporary paths; never commit author databases, credentials, generated native results, or backups.
- Proposed commands are not evidence. Record native WebView2 text input, accessibility, persistence, provider, and packaging evidence in the qualification documents before claiming support.
