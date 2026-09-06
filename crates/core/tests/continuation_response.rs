use serde_json::json;
use webnovel_core::context::continuation::{
    CONTINUATION_RESPONSE_CONTRACT, MAX_CONTINUATION_RESPONSE_BYTES, validate_continuation_output,
    validate_continuation_paragraphs,
};

fn response(paragraphs: Vec<&str>) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": CONTINUATION_RESPONSE_CONTRACT,
        "suggestions": [{
            "title": "Continue",
            "paragraphs": paragraphs,
            "explanation": "A bounded continuation."
        }]
    }))
    .unwrap()
}

#[test]
fn accepts_exact_single_candidate_and_preserves_unicode_text() {
    let raw = response(vec!["  First paragraph.  ", "第二段 👩‍🚀"]);
    let output = validate_continuation_output(&raw).unwrap();
    assert_eq!(output.schema_version, CONTINUATION_RESPONSE_CONTRACT);
    assert_eq!(output.suggestions.len(), 1);
    assert_eq!(output.suggestions[0].paragraphs[0], "  First paragraph.  ");
    assert_eq!(output.suggestions[0].paragraphs[1], "第二段 👩‍🚀");
}

#[test]
fn rejects_wrong_schema_multiple_candidates_unknown_fields_and_blank_title() {
    let mut wrong = serde_json::from_str::<serde_json::Value>(&response(vec!["one"])).unwrap();
    wrong["schemaVersion"] = json!("continuation-output.v0");
    assert!(validate_continuation_output(&wrong.to_string()).is_err());

    let mut multiple = serde_json::from_str::<serde_json::Value>(&response(vec!["one"])).unwrap();
    multiple["suggestions"] = json!([
        {"title":"one","paragraphs":["one"],"explanation":""},
        {"title":"two","paragraphs":["two"],"explanation":""}
    ]);
    assert!(validate_continuation_output(&multiple.to_string()).is_err());

    let mut unknown = serde_json::from_str::<serde_json::Value>(&response(vec!["one"])).unwrap();
    unknown["suggestions"][0]["unexpected"] = json!(true);
    assert!(validate_continuation_output(&unknown.to_string()).is_err());

    let mut blank = serde_json::from_str::<serde_json::Value>(&response(vec!["one"])).unwrap();
    blank["suggestions"][0]["title"] = json!("   ");
    assert!(validate_continuation_output(&blank.to_string()).is_err());
}

#[test]
fn enforces_paragraph_count_lines_utf16_and_total_limits() {
    assert!(validate_continuation_paragraphs(&[]).is_err());
    assert!(validate_continuation_paragraphs(&[" ".into()]).is_err());
    assert!(validate_continuation_paragraphs(&["line\nbreak".into()]).is_err());
    assert!(validate_continuation_paragraphs(&["a".repeat(8193)]).is_err());
    assert!(validate_continuation_paragraphs(&["a".repeat(8192), "b".repeat(8192)]).is_ok());
    assert!(validate_continuation_paragraphs(&["a".repeat(100_001)]).is_err());
    let too_many = (0..129).map(|_| "p".to_owned()).collect::<Vec<_>>();
    assert!(validate_continuation_paragraphs(&too_many).is_err());
}

#[test]
fn rejects_raw_responses_over_the_64_kib_limit() {
    let padding = "x".repeat(MAX_CONTINUATION_RESPONSE_BYTES);
    let raw = format!(
        "{{\"schemaVersion\":\"{}\",\"suggestions\":[{{\"title\":\"x\",\"paragraphs\":[\"{}\"],\"explanation\":\"\"}}]}}",
        CONTINUATION_RESPONSE_CONTRACT, padding
    );
    assert!(raw.len() > MAX_CONTINUATION_RESPONSE_BYTES);
    assert!(validate_continuation_output(&raw).is_err());
}
