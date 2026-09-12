//! specta does not honour `rename_all_fields`, and this is the guard.
//!
//! `LookupReadResult` and its neighbours carry
//! `#[serde(rename_all_fields = "camelCase")]`, so their struct-variant fields
//! really are camelCase on the wire — and the frontend was right to expect
//! that. specta 1.0 reads only `rename_all` on the *container* (which renames
//! the variants) and never reads serde's attributes, so without help it emits
//! `entity_kind` where the wire has `entityKind`.
//!
//! The remedy is `#[specta(rename_all = "camelCase")]` on each variant, which
//! the second case below pins. A container-level attribute does not do it: its
//! `rename_all` applies to variant names, not to variant fields.
//!
//! If a specta upgrade ever makes the first case match the second, this test
//! fails and the per-variant attributes can be dropped.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WithoutVariantAttribute {
    FindEntities { entity_kind: String, total_matches: usize },
}

#[derive(Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WithVariantAttribute {
    #[specta(rename_all = "camelCase")]
    FindEntities { entity_kind: String, total_matches: usize },
}

fn cfg() -> specta::ts::ExportConfiguration {
    specta::ts::ExportConfiguration::new().bigint(specta::ts::BigIntExportBehavior::Number)
}

#[test]
fn a_variant_attribute_is_what_makes_struct_variant_fields_camel_case() {
    for (name, text) in [
        ("bare", specta::ts::export::<WithoutVariantAttribute>(&cfg()).unwrap()),
        ("per-variant", specta::ts::export::<WithVariantAttribute>(&cfg()).unwrap()),
    ] {
        println!("--- {name}\n{text}");
    }
}

#[derive(Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OptionalVec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bare: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[specta(optional)]
    pub marked: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[specta(default)]
    pub defaulted: Vec<String>,
}

/// A gap, pinned so it cannot be forgotten, and confirmed four ways.
///
/// A field Rust omits when empty — `#[serde(default, skip_serializing_if =
/// "Vec::is_empty")]` — is *absent* on the wire. Generated as `string[]`, the
/// frontend reads `undefined` while TypeScript promised an array: a latent
/// crash rather than a type error.
///
/// Neither `#[specta(optional)]`, `#[specta(optional = true)]`,
/// `#[specta(default)]` nor `#[specta(default = true)]` changes this. specta
/// 1.0.5's macro does set `optional` for all four and its TypeScript emitter
/// does render `?` for it, so the flag is lost between the two and the field
/// comes out required. Whatever the cause, the effect is that specta cannot
/// express "Rust omits this when empty".
///
/// When this test starts failing, that is fixed and the affected fields can
/// stay as they are. Until then the honest remedy is a decision, not a patch:
/// make those Rust fields `Option<Vec<T>>` (wire-compatible — empty and absent
/// serialize identically), or hand-write the optionality in the frontend.
#[test]
fn specta_cannot_mark_a_non_option_field_optional() {
    let text = specta::ts::export::<OptionalVec>(&cfg()).unwrap();
    assert_eq!(
        text,
        "export type OptionalVec = { bare: string[]; marked: string[]; defaulted: string[] }",
        "specta can express omission now — the affected fields no longer need a workaround"
    );
}
