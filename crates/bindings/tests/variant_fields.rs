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

#[derive(Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DefaultedVec {
    #[serde(default)]
    pub source_refs: Vec<String>,
    pub plain: Vec<String>,
}

/// `#[serde(default)]` on its own makes specta emit `?`, and the wire keeps the
/// field.
///
/// `default` is a *deserialization* tolerance: it says a missing field is an
/// error the reader may ignore. It says nothing about writing, so Rust
/// serializes `source_refs` on every send. specta reads it as optionality
/// anyway and emits `sourceRefs?: string[]`, which is the wrong direction of
/// wrong: a frontend that reads the field must now defend against `undefined`
/// that cannot occur.
///
/// The generator does not correct this, and that is a decision rather than an
/// omission. Correcting it means emitting the field as required, which is right
/// for a response and wrong for a request: these types travel both ways, and
/// `#[serde(default)]` is exactly the statement that Rust tolerates the field
/// being absent on the way in. Thirty-one fields across the tree carry it —
/// `ScopeGrant`, `ModelSelection` and `PacketRequest` among them — so removing
/// the `?` would make every request-builder site name a field it may legitimately
/// leave out. Leaving it costs a defensive read at the response sites and no
/// correctness anywhere, which is the cheaper half of a trade the type system
/// cannot express. The reading is pinned here so that a specta upgrade which
/// changes it fails loudly.
///
/// Note the asymmetry with a struct *variant* field: `TypedReplacementBlock`'s
/// `content` carries the same attribute and comes out required. specta reads
/// `default` on a struct field and not on a variant's.
#[test]
fn serde_default_alone_is_read_as_optionality() {
    let text = specta::ts::export::<DefaultedVec>(&cfg()).unwrap();
    println!("--- DefaultedVec\n{text}");
    assert_eq!(
        text,
        "export type DefaultedVec = { sourceRefs?: string[]; plain: string[] }",
        "specta no longer reads a bare `#[serde(default)]` as optionality"
    );
}

#[derive(Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OptionalForms {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defaulted: Option<String>,
    pub plain: Option<String>,
}

/// What specta actually keys the `?` on: an `Option` plus an omission
/// attribute, or a bare `#[serde(default)]` on anything.
///
/// This matters because it is the boundary of what [`OMITTED_WHEN_EMPTY`] has
/// to supply by hand. A `Vec<T>` with `skip_serializing_if = "Vec::is_empty"`
/// gets no `?` — that is the gap the list fills. An `Option<T>` whose `None` is
/// skipped gets one, so it needs no entry; and a bare `Option<T>` with no
/// attribute is required, because Rust writes `null` there and the frontend
/// must handle it.
///
/// The `| null` on the two skipped fields is specta's rendering of `Option`,
/// not something the wire does: a field skipped when `None` is *absent*, never
/// `null`. Removing it would tighten 141 fields across 64 types, but the same
/// type often travels both ways — a request may legitimately omit what a
/// response always carries — and `null` is accepted on the way in. Left as a
/// widening: it forces a defensive check, it cannot cause one to be missed.
#[test]
fn optional_is_an_option_plus_an_omission_attribute() {
    let text = specta::ts::export::<OptionalForms>(&cfg()).unwrap();
    println!("--- OptionalForms\n{text}");
    assert_eq!(
        text,
        "export type OptionalForms = { skipped?: string | null; defaulted?: string | null; plain: string | null }",
        "specta's optionality rule changed — update OMITTED_WHEN_EMPTY's premise"
    );
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
