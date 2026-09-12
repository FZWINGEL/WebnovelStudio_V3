//! Read-only bridge from an existing Codex ChatGPT login to app-server.
//!
//! The application never imports a Codex credential into its settings or
//! database.  This module reads only the two values needed for the unstable
//! `chatgptAuthTokens` login shape, keeps the access token in a redacted
//! [`SecretValue`], and produces the login frame only at the private stdio
//! boundary.  Refresh tokens, id tokens, API keys, and all other fields are
//! deliberately ignored or refused.

use crate::credentials::{MAX_CREDENTIAL_BYTES, SecretValue};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use wns_kernel::{CoreError, CoreResult};

/// The auth file is not an application database.  Keep its temporary parse
/// buffer bounded even when a malformed file has been placed at the path.
pub const MAX_AUTH_FILE_BYTES: usize = 64 * 1024;
const MAX_ACCOUNT_ID_BYTES: usize = 256;

#[derive(Debug, Deserialize)]
struct AuthFile {
    tokens: Option<AuthTokens>,
}

#[derive(Debug, Deserialize)]
struct AuthTokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

/// A memory-only account handoff.  The type intentionally has no `Debug`,
/// `Serialize`, `Clone`, or public access-token accessor.
pub struct ExternalAuth {
    access_token: SecretValue,
    account_id: String,
    account_hash: String,
}

impl ExternalAuth {
    /// The account identity is safe to use for runtime fencing.  It is a
    /// SHA-256 digest and cannot be used as an authentication credential.
    pub fn account_hash(&self) -> &str {
        &self.account_hash
    }

    /// Session-only fence. Never store this token-derived value in a packet,
    /// settings, or logs; a changed login requires a checked replacement server.
    #[cfg(windows)]
    pub(crate) fn token_fingerprint(&self) -> String {
        sha256_hex(self.access_token.expose_bytes())
    }

    pub(crate) fn login_method() -> &'static str {
        "account/login/start"
    }

    /// Build the unstable external-auth params for the driver-owned RPC
    /// boundary.  The returned value must not be logged, persisted, or kept
    /// after the login response is settled.
    pub(crate) fn login_params(&self) -> CoreResult<Value> {
        Ok(serde_json::json!({
            "type": "chatgptAuthTokens",
            "accessToken": std::str::from_utf8(self.access_token.expose_bytes()).map_err(|_| auth_error(
                "InvalidCodexAuth",
                "The Codex access token is not valid UTF-8.",
            ))?,
            "chatgptAccountId": self.account_id,
            "chatgptPlanType": Value::Null,
        }))
    }
}

/// Read the author's configured Codex auth file without touching any keyring
/// or writing to the author's Codex home.  The caller supplies the discovered
/// path in production; this helper only resolves the conventional location.
pub fn read_discovered_auth() -> CoreResult<ExternalAuth> {
    let home = discovered_codex_home()?;
    read_auth_file(&home.join("auth.json"))
}

/// Resolve the author Codex home without creating it or reading its contents.
/// `CODEX_HOME` is preferred; otherwise the platform's user profile home is
/// used.  An empty or relative override is rejected to avoid ambiguous paths.
pub fn discovered_codex_home() -> CoreResult<PathBuf> {
    if let Some(value) = std::env::var_os("CODEX_HOME") {
        let path = PathBuf::from(value);
        if !path.is_absolute() || path.as_os_str().is_empty() {
            return Err(auth_error(
                "CodexExternalAuthUnavailable",
                "The configured Codex home is invalid.",
            ));
        }
        return Ok(path);
    }

    #[cfg(windows)]
    let profile = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let profile = std::env::var_os("HOME");
    profile
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && !path.as_os_str().is_empty())
        .map(|path| path.join(".codex"))
        .ok_or_else(|| {
            auth_error(
                "CodexExternalAuthUnavailable",
                "The author's Codex home could not be determined.",
            )
        })
}

/// Read and validate a synthetic or author-provided auth file.  Production
/// callers must pass the author home's `auth.json`; this function does not
/// search elsewhere or fall back to API-key or keyring authentication.
pub fn read_auth_file(path: &Path) -> CoreResult<ExternalAuth> {
    let mut bytes = read_bounded(path)?;
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => {
            wipe_bytes(&mut bytes);
            return Err(auth_error(
                "CodexExternalAuthUnavailable",
                "The system clock is invalid.",
            ));
        }
    };
    let result = parse_auth_bytes_at(&bytes, now);
    wipe_bytes(&mut bytes);
    result
}

fn read_bounded(path: &Path) -> CoreResult<Vec<u8>> {
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|_| {
        auth_error(
            "CodexExternalAuthUnavailable",
            "The author's Codex auth file is unavailable. Sign in to Codex and try again.",
        )
    })?;
    let mut bytes = Vec::new();
    if file
        .take((MAX_AUTH_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        wipe_bytes(&mut bytes);
        return Err(auth_error(
            "CodexExternalAuthUnavailable",
            "The author's Codex auth file could not be read.",
        ));
    }
    if bytes.len() > MAX_AUTH_FILE_BYTES {
        wipe_bytes(&mut bytes);
        return Err(auth_error(
            "CodexExternalAuthUnavailable",
            "The author's Codex auth file is too large.",
        ));
    }
    Ok(bytes)
}

fn parse_auth_bytes_at(bytes: &[u8], now: u64) -> CoreResult<ExternalAuth> {
    if bytes.is_empty() || bytes.len() > MAX_AUTH_FILE_BYTES {
        return Err(auth_error(
            "CodexExternalAuthUnavailable",
            "The author's Codex auth file is empty or too large.",
        ));
    }
    let parsed: AuthFile = serde_json::from_slice(bytes).map_err(|_| {
        auth_error(
            "CodexExternalAuthUnavailable",
            "The author's Codex auth file is malformed.",
        )
    })?;
    let Some(tokens) = parsed.tokens else {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "Codex API-key or keyring-only authentication cannot be handed to app-server.",
        ));
    };
    let Some(access_token) = tokens.access_token else {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex auth file does not contain a ChatGPT access token.",
        ));
    };
    let Some(account_id) = tokens.account_id else {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex auth file does not contain a ChatGPT account ID.",
        ));
    };
    validate_account_id(&account_id)?;
    validate_jwt(&access_token, now)?;
    let access_token = SecretValue::new(access_token.into_bytes())?;
    let account_hash = sha256_hex(account_id.as_bytes());
    Ok(ExternalAuth {
        access_token,
        account_id,
        account_hash,
    })
}

fn validate_account_id(account_id: &str) -> CoreResult<()> {
    if account_id.is_empty()
        || account_id.len() > MAX_ACCOUNT_ID_BYTES
        || account_id.chars().any(char::is_control)
    {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex account ID is invalid.",
        ));
    }
    Ok(())
}

fn validate_jwt(token: &str, now: u64) -> CoreResult<()> {
    if token.len() > MAX_CREDENTIAL_BYTES || token.chars().any(char::is_control) {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token is invalid.",
        ));
    }
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token is not a JWT.",
        ));
    };
    if parts.next().is_some() || header.is_empty() || payload.is_empty() || signature.is_empty() {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token is not a valid JWT.",
        ));
    }
    if decode_base64url(signature).is_none() {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token has an invalid JWT signature.",
        ));
    }
    let header = decode_base64url(header).ok_or_else(|| {
        auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token has an invalid JWT header.",
        )
    })?;
    let header: Value = serde_json::from_slice(&header).map_err(|_| {
        auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token has an invalid JWT header.",
        )
    })?;
    if !header.is_object() {
        return Err(auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token has an invalid JWT header.",
        ));
    }
    let payload = decode_base64url(payload).ok_or_else(|| {
        auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token has an invalid JWT payload.",
        )
    })?;
    let claims: Value = serde_json::from_slice(&payload).map_err(|_| {
        auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token has an invalid JWT payload.",
        )
    })?;
    let expires = claims.get("exp").and_then(Value::as_u64).ok_or_else(|| {
        auth_error(
            "CodexExternalAuthUnsupported",
            "The Codex access token does not contain an expiration.",
        )
    })?;
    if expires <= now {
        return Err(auth_error(
            "CodexExternalAuthExpired",
            "The Codex access token has expired. Sign in again and try again.",
        ));
    }
    Ok(())
}

fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits) - 1;
        }
    }
    if bits >= 6 || (bits > 0 && accumulator != 0) {
        return None;
    }
    Some(output)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn wipe_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        // Volatile writes prevent the compiler from removing this best-effort
        // cleanup of the temporary JSON buffer.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

fn auth_error(code: &'static str, message: &'static str) -> CoreError {
    CoreError::new(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_TOKEN: &str = "eyJhbGciOiJub25lIn0.eyJleHAiOjQxMDI0NDQ4MDB9.ZmFrZQ";

    fn auth_json(token: &str, account: &str) -> Vec<u8> {
        serde_json::json!({
            "last_refresh": "2026-09-07T00:00:00Z",
            "tokens": {
                "access_token": token,
                "id_token": "ignored",
                "refresh_token": "must-not-be-read",
                "account_id": account
            },
            "unrelated": {"secret": "ignored"}
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn accepts_only_the_access_token_and_account_identity() {
        let auth = parse_auth_bytes_at(&auth_json(VALID_TOKEN, "acct-fixture"), 1_700_000_000)
            .expect("synthetic fixture is valid");
        assert_eq!(auth.account_hash().len(), 64);
        let text = serde_json::to_string(&auth.login_params().expect("params")).expect("json");
        assert_eq!(ExternalAuth::login_method(), "account/login/start");
        assert!(text.contains("chatgptAuthTokens"));
        assert!(text.contains(VALID_TOKEN));
        assert!(text.contains("acct-fixture"));
        assert!(text.contains("chatgptPlanType"));
    }

    #[test]
    fn rejects_expired_and_malformed_tokens() {
        let expired = "eyJhbGciOiJub25lIn0.eyJleHAiOjF9.ZmFrZQ";
        assert_eq!(
            parse_auth_bytes_at(&auth_json(expired, "acct"), 2)
                .err()
                .unwrap()
                .code,
            "CodexExternalAuthExpired"
        );
        for token in ["not-a-jwt", "a.b.c", "a.!!!.c"] {
            assert_eq!(
                parse_auth_bytes_at(&auth_json(token, "acct"), 1_700_000_000)
                    .err()
                    .unwrap()
                    .code,
                "CodexExternalAuthUnsupported"
            );
        }
    }

    #[test]
    fn rejects_missing_identity_api_key_and_oversized_files() {
        let missing_account = auth_json(VALID_TOKEN, "");
        assert!(parse_auth_bytes_at(&missing_account, 1_700_000_000).is_err());
        let api_key = br#"{"OPENAI_API_KEY":"sk-fixture"}"#;
        assert_eq!(
            parse_auth_bytes_at(api_key, 1_700_000_000)
                .err()
                .unwrap()
                .code,
            "CodexExternalAuthUnsupported"
        );
        assert_eq!(
            parse_auth_bytes_at(&vec![b'x'; MAX_AUTH_FILE_BYTES + 1], 1_700_000_000)
                .err()
                .unwrap()
                .code,
            "CodexExternalAuthUnavailable"
        );
    }
}
