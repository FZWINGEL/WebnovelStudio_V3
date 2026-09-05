use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use webnovel_core::documents::validate_scope_json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureFile {
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    name: String,
    request: Value,
    valid: bool,
    error_contains: Option<String>,
}

#[test]
fn shared_scope_fixtures_match() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/fixtures/w1_scope_golden.json");
    let fixture: FixtureFile =
        serde_json::from_str(&fs::read_to_string(path).expect("read W1 scope fixture"))
            .expect("parse W1 scope fixture");
    for case in fixture.cases {
        let result =
            validate_scope_json(&serde_json::to_string(&case.request).expect("encode request"));
        if case.valid {
            assert!(
                result.is_ok(),
                "fixture {} failed: {:?}",
                case.name,
                result.err()
            );
        } else {
            let error = result.expect_err(&case.name);
            if let Some(expected) = case.error_contains {
                assert!(
                    error.contains(&expected),
                    "fixture {} error {error:?} does not contain {expected:?}",
                    case.name
                );
            }
        }
    }
}
