//! The cross-language contract, shipped as data.
//!
//! Four golden fixtures describe the same bytes to Rust and to TypeScript. They
//! used to be read at *runtime* by relative path — `../../contracts/fixtures/`
//! from a test, `../../../` from another — which makes the path a function of
//! the working directory and breaks the moment a consumer moves a directory
//! deeper. Here they are compiled in with `include_str!`, so the path is
//! resolved once by the compiler, relative to this file, and a consumer writes
//! `contracts::W0_SNAPSHOT_GOLDEN`.
//!
//! The files themselves stay in `contracts/fixtures/`, which is where the
//! frontend's `import golden from '../../../../contracts/fixtures/…'` already
//! points. One copy, two languages, no duplication.

/// The editor's restricted-snapshot contract: accepted and rejected inputs.
pub const W0_SNAPSHOT_GOLDEN: &str = include_str!("../fixtures/w0_snapshot_golden.json");

/// The W1 append-scope contract, case by case.
pub const W1_SCOPE_GOLDEN: &str = include_str!("../fixtures/w1_scope_golden.json");

/// A save receipt, byte for byte, as both sides must hash it.
pub const W2_SAVE_RECEIPT: &str = include_str!("../fixtures/w2_save_receipt.json");

/// The structured-proposal contract: typed replacement blocks.
pub const STRUCTURED_PROPOSALS_GOLDEN: &str =
    include_str!("../fixtures/structured_proposals_golden.json");
