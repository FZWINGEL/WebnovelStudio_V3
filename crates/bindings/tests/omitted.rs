//! Independently derive the omission list from parsed Rust source.

mod support;

#[test]
fn the_omitted_list_matches_the_rust_source() {
    let source = support::omitted_fields(false);
    let listed = wns_bindings::OMITTED_WHEN_EMPTY
        .iter()
        .map(|(owner, field)| (owner.to_string(), field.to_string()))
        .collect();
    assert_eq!(
        source, listed,
        "update OMITTED_WHEN_EMPTY to match Rust serialization"
    );
}
