# CI audit evidence

Audit date: 7 September 2026.
Repository snapshot: FZWINGEL/WebnovelStudio_V3 at c83a127423512b0963eefebb992fefe719280cb2.

`baseline.json` transcribes measured job-step and Rust-test log timings from Actions run 34067931031, Windows job 101579887315. It is one sample, not a median.

`cleanup-reproduction.mjs` is a standalone, owned-child-process experiment. Run `node cleanup-reproduction.mjs`. It intentionally reproduces one five-second delay before demonstrating the already-exited guard. `cleanup-reproduction.json` is the observed output on Linux and Node v22.16.0. The project pins Node v24.20.0 and runs native CI on Windows. This reproduction is not a native CI benchmark and its guard is not a complete process-tree cleanup implementation.

No credentials, author databases, repository modifications, or live-provider requests are included.
