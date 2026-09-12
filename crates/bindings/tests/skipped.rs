//! Independently derive the omission list from parsed Rust source.

mod support;

#[test]
fn the_skipped_list_matches_the_rust_source() {
    let source = support::omitted_fields(true);
    let listed = wns_bindings::SKIPPED_WHEN_NONE
        .iter()
        .map(|(owner, field)| (owner.to_string(), field.to_string()))
        .collect();
    assert_eq!(
        source, listed,
        "update SKIPPED_WHEN_NONE to match Rust serialization"
    );
}
