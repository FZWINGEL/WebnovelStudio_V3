//! The TypeScript bindings for every type that crosses the IPC boundary.
//!
//! Rust is the source of truth. The frontend used to carry a hand-written
//! mirror of each type — 255 of them across the `src/ipc/` modules — and
//! nothing compared the two, so a renamed Rust field became a runtime
//! `undefined` in the renderer with no failing test anywhere. That is D6.
//!
//! This crate is the single collector. It is not a layer: it depends on all of
//! them and nothing depends on it, so it is deliberately absent from the
//! layer table, like `wns-app` and `contracts`. Each crate supplies its own
//! [`Group`]; [`render`] turns them into one TypeScript file per crate, and
//! `tests/drift.rs` fails the build when the committed output is stale.
//!
//! Adding a crate is one `Group` and one line in `groups()` — with two things
//! I learned attempting the second crate and reverting it:
//!
//! * **The group must list the whole closure, not the crate's own types.** A
//!   workshop snapshot reaches `DiscussionRun`, which reaches the run
//!   vocabulary, which reaches the provider and context vocabularies. `export`
//!   renders a type and *references* the named types it depends on; it does not
//!   always declare them, so every name the generated file mentions has to be
//!   in the list. The frontend's compiler names exactly which ones are missing,
//!   which is the reliable way to close the list.
//! * **"Public" is not "crosses the wire."** A blanket pass that derives
//!   `specta::Type` on every `pub struct` also hits internal accumulators — the
//!   provider parser state — and those hold private types that cannot be
//!   derived at all. The wire surface is a choice per type; the kernel's eleven
//!   needed no skips because each was chosen.
//!
//! * **A group carries a frontend tail, and it is where the value is.** The
//!   workshop group converged at 131 types, and replacing its mirrors then
//!   failed to typecheck in ~57 places — not because the generation was wrong,
//!   but because the mirrors were. `BasisKind` and `ContinuationBasis` are
//!   distinct Rust enums the hand-written TypeScript used one name for, and
//!   several fields the mirrors declared optional are required in Rust. That is
//!   D6 becoming visible the moment something can see it. A group is finished
//!   when the frontend compiles against it, not when the file is generated; the
//!   kernel group had no tail because its eleven types were already exact.
//!
//! * **specta does not honour `rename_all_fields`, and that is the largest
//!   error class.** `LookupReadResult` and its neighbours carry
//!   `#[serde(rename_all_fields = "camelCase")]`, so their struct-variant fields
//!   really are camelCase on the wire — and the frontend was right to expect
//!   that. specta 1.0 reads only `rename_all` on the *container* and never reads
//!   serde's attributes at all, so it emitted `entity_kind` where the wire has
//!   `entityKind`. The remedy is `#[specta(rename_all = "camelCase")]` on each
//!   variant — settled against a minimal case in `tests/variant_fields.rs`, and
//!   it works. A container-level attribute does not: its `rename_all` applies to
//!   variant *names*. Applying this across the tree took the workshop group's
//!   frontend tail from 83 errors to 24 in one step, which is most of what that
//!   tail was.
//!
//! * **A field Rust omits when empty cannot be expressed, and that is the last
//!   blocker.** `#[serde(default, skip_serializing_if = "Vec::is_empty")]` means
//!   the field is *absent* on the wire, so the frontend reads `undefined` where
//!   the generated type promises an array. That is a latent crash, not a type
//!   error. `#[specta(optional)]` marks `Option` fields and does nothing for the
//!   rest — `tests/variant_fields.rs` pins both halves. The honest fixes are to
//!   make those Rust fields `Option<Vec<T>>`, or to hand-write the optionality
//!   in the frontend; either is a decision to take deliberately, and it is what
//!   stands between the workshop group's remaining 24 errors and zero.
//!
//! **Let the compiler close both lists.** The workshop's derives converged in 6
//!   rounds (workshop → story vocabulary → run vocabulary → provider and context
//!   vocabularies) and the group's names in 6 more against `tsc`. Neither list
//!   should be typed by hand, and both converge quickly when the gap is read
//!   from the error output.
//!
//! ## What the frontend does with a narrower type
//!
//! Rust carries a document body as `serde_json::Value`, so specta emits `any`.
//! The editor knows the body is a `WnsDocument` and says so once, as
//! `Omit<WireRevision, 'body'> & { body: WnsDocument }`. That is narrower than
//! the mirror it replaces: every *other* field still comes from Rust, so a
//! rename there is still a build failure rather than a runtime `undefined`.

use std::collections::BTreeMap;

/// One crate's IPC-facing types.
pub struct Group {
    /// The generated file's name, without extension.
    pub file: &'static str,
    /// The crate's own name, for the file header.
    pub crate_name: &'static str,
    /// Each entry is the declared TypeScript name and the emitted text for it.
    pub types: Vec<(String, String)>,
}

/// Export one type. `specta` emits it *and* everything it references.
pub fn one<T: specta::NamedType>() -> Result<String, specta::ts::TsExportError> {
    specta::ts::export::<T>(&config())
}

fn config() -> specta::ts::ExportConfiguration {
    specta::ts::ExportConfiguration::new().bigint(specta::ts::BigIntExportBehavior::Number)
}

/// Build a group from a list of exported types, keeping one declaration per
/// name — `Head` sits inside several, and exporting each type in turn repeats
/// it.
pub fn group(
    file: &'static str,
    crate_name: &'static str,
    exported: Vec<(&'static str, Result<String, specta::ts::TsExportError>)>,
) -> Result<Group, specta::ts::TsExportError> {
    let mut declarations: BTreeMap<String, String> = BTreeMap::new();
    for (_, text) in exported {
        for block in split_declarations(&text?) {
            if let Some(name) = declaration_name(&block) {
                declarations.entry(name).or_insert(block);
            }
        }
    }
    Ok(Group { file, crate_name, types: declarations.into_iter().collect() })
}

/// Render one group as a TypeScript module.
pub fn render(group: &Group) -> String {
    let mut out = format!(
        "// Generated from `{}` by `crates/bindings`. Do not edit.\n\
         // Change the Rust type and run `cargo run -p wns-bindings`.\n\n",
        group.crate_name
    );
    for (_, block) in &group.types {
        out.push_str(block.trim_end());
        out.push_str("\n\n");
    }
    out
}

/// Where the generated files live, relative to the workspace root.
pub const OUTPUT_DIR: &str = "apps/desktop/src/ipc/generated";

/// One declaration per element. A declaration starts at `export`, and a doc
/// comment starts the block it belongs to.
fn split_declarations(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let starts_item = line.starts_with("export ") || line.starts_with("/**");
        if starts_item && current.contains("export ") {
            blocks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        blocks.push(current);
    }
    blocks
}

fn declaration_name(block: &str) -> Option<String> {
    let start = block.find("export ")?;
    let rest = block[start..].strip_prefix("export ")?;
    let rest = rest
        .strip_prefix("type ")
        .or_else(|| rest.strip_prefix("enum "))
        .or_else(|| rest.strip_prefix("interface "))?;
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

/// Every group the frontend has been migrated onto.
pub fn groups() -> Result<Vec<Group>, specta::ts::TsExportError> {
    Ok(vec![wns_groups::kernel()?])
}

/// The workspace root, from this crate's manifest directory.
pub fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

mod wns_groups {
    use super::{group, one, Group};

    /// `wns-kernel`'s IPC types.
    pub fn kernel() -> Result<Group, specta::ts::TsExportError> {
        group(
            "kernel",
            "wns-kernel",
            vec![
                ("CoreError", one::<wns_kernel::CoreError>()),
                ("Head", one::<wns_kernel::Head>()),
                ("ProjectAccess", one::<wns_kernel::ProjectAccess>()),
                ("Revision", one::<wns_kernel::Revision>()),
                ("ProjectInfo", one::<wns_kernel::ProjectInfo>()),
                ("DocumentRecord", one::<wns_kernel::DocumentRecord>()),
                ("DocumentRole", one::<wns_kernel::DocumentRole>()),
                ("RestoredDecision", one::<wns_kernel::RestoredDecision>()),
                ("AppliedDecision", one::<wns_kernel::AppliedDecision>()),
                ("StoredResult", one::<wns_kernel::StoredResult>()),
                ("SnapshotReceipt", one::<wns_kernel::SnapshotReceipt>()),
            ],
        )
    }
}
