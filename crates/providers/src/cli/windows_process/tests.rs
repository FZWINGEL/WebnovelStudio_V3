use super::{EnvironmentBlock, EnvironmentPolicy, owned_handle, quote_windows_arg};
use std::collections::BTreeMap;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

#[test]
fn rejects_both_windows_invalid_handle_forms() {
    assert!(unsafe { owned_handle(std::ptr::null_mut()) }.is_err());
    assert!(unsafe { owned_handle(INVALID_HANDLE_VALUE) }.is_err());
}

#[test]
fn quotes_spaces_quotes_and_trailing_backslashes() {
    assert_eq!(
        String::from_utf16(&quote_windows_arg(&[])).expect("valid UTF-16"),
        "\"\""
    );
    assert_eq!(
        String::from_utf16(&quote_windows_arg(
            &"plain".encode_utf16().collect::<Vec<_>>()
        ))
        .expect("valid UTF-16"),
        "plain"
    );
    assert_eq!(
        String::from_utf16(&quote_windows_arg(
            &"a b\\".encode_utf16().collect::<Vec<_>>()
        ))
        .expect("valid UTF-16"),
        "\"a b\\\\\""
    );
    assert_eq!(
        String::from_utf16(&quote_windows_arg(
            &"a\"b".encode_utf16().collect::<Vec<_>>()
        ))
        .expect("valid UTF-16"),
        "\"a\\\"b\""
    );
}

#[test]
fn explicit_empty_environment_is_double_nul_terminated() {
    let block = EnvironmentBlock::new(&EnvironmentPolicy::Explicit(BTreeMap::new()))
        .expect("empty environment");
    assert_eq!(block.values, Some(vec![0, 0]));
}
