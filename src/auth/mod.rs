use crate::{paths::AppPaths, AppConfig};
use aes::Aes128;
use anyhow::{Context, Result};
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use std::path::Path;
use url::Url;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;

const ENTROPY: [u8; 16] = [
    200, 118, 244, 174, 76, 149, 46, 254, 242, 250, 15, 84, 25, 192, 156, 67,
];
const SALT: &[u8] = b"someSalt";
const ITERATIONS: u32 = 1000;
const TOKEN_CIPHERTEXT_LEN: usize = 0x30;
pub fn extract_token_from_uri(uri: &str) -> Result<String> {
    if let Ok(url) = Url::parse(uri) {
        for (_, value) in url.query_pairs() {
            if looks_like_token(&value) {
                return Ok(value.into_owned());
            }
        }
    }

    find_token_candidate(uri).context("no Hearthstone login token found in callback URI")
}

// Token shape: <2-char region>-<32-char session key>-<account id>
// The account id used to be hardcoded at 9 digits (total length 45), but
// older Blizzard accounts have shorter numeric ids (e.g. 8 digits), which
// made `looks_like_token` reject perfectly valid tokens. The head (region +
// dash + session key + dash) is always exactly 36 bytes; only the trailing
// account id segment varies in length.
const TOKEN_HEAD_LEN: usize = 36;
const TOKEN_TAIL_MIN: usize = 6;
// Must stay <= 11: with TOKEN_HEAD_LEN = 36, a total token length above 47
// pushes AES-CBC/PKCS7 padding into an extra 16-byte block, which would
// break the `TOKEN_CIPHERTEXT_LEN == 0x30` check in `encrypt_token_for_user`.
const TOKEN_TAIL_MAX: usize = 11;

pub fn looks_like_token(value: &str) -> bool {
    token_match_len(value.as_bytes()) == Some(value.len())
}

/// Returns the length of a token if `bytes` starts with one, or `None`.
fn token_match_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < TOKEN_HEAD_LEN + TOKEN_TAIL_MIN {
        return None;
    }
    if bytes[2] != b'-' || bytes[35] != b'-' {
        return None;
    }
    let is_alnum = |range: std::ops::Range<usize>| {
        bytes[range].iter().all(u8::is_ascii_alphanumeric)
    };
    if !is_alnum(0..2) || !is_alnum(3..35) {
        return None;
    }

    let mut tail_len = 0;
    while tail_len < TOKEN_TAIL_MAX
        && bytes.get(TOKEN_HEAD_LEN + tail_len).is_some_and(u8::is_ascii_alphanumeric)
    {
        tail_len += 1;
    }
    if tail_len < TOKEN_TAIL_MIN {
        return None;
    }

    // Don't stop in the middle of a longer alphanumeric run than we allow,
    // otherwise a coincidental match could swallow only part of a longer id.
    if bytes
        .get(TOKEN_HEAD_LEN + tail_len)
        .is_some_and(u8::is_ascii_alphanumeric)
    {
        return None;
    }

    Some(TOKEN_HEAD_LEN + tail_len)
}

fn find_token_candidate(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if bytes.len() < TOKEN_HEAD_LEN + TOKEN_TAIL_MIN {
        return None;
    }

    for start in 0..bytes.len() {
        if let Some(len) = token_match_len(&bytes[start..]) {
            return Some(input[start..start + len].to_string());
        }
    }
    None
}

pub fn write_encrypted_token_for_current_user(path: &Path, token: &str) -> Result<()> {
    let username = current_username();
    let encrypted = encrypt_token_for_user(token, &username)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, encrypted).with_context(|| format!("failed to write {}", path.display()))
}

pub fn handle_callback_uri(paths: &AppPaths, uri: &str) -> Result<()> {
    let mut config = AppConfig::load_or_default(&paths.config_file)?;
    let game_dir = config.game_dir.clone().unwrap_or(paths.game_dir.clone());
    let token = extract_token_from_uri(uri)?;
    let token_path = game_dir.join("token");
    tracing::info!(
        game_dir = %game_dir.display(),
        token_path = %token_path.display(),
        "writing login token from auth callback"
    );
    write_encrypted_token_for_current_user(&token_path, &token)?;
    config.game_dir = Some(game_dir);
    config.logged_in = true;
    config.last_login_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string(),
    );
    config.save(&paths.config_file)?;
    tracing::info!(token_path = %token_path.display(), "login token written");
    Ok(())
}

pub fn encrypt_token_for_user(token: &str, username: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(looks_like_token(token), "token format is invalid");

    let key = encryption_key_for_user(username);
    let iv = [0u8; 16];
    let ciphertext = Aes128CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(token.as_bytes());

    anyhow::ensure!(
        ciphertext.len() == TOKEN_CIPHERTEXT_LEN,
        "unexpected encrypted token length {}",
        ciphertext.len()
    );
    Ok(ciphertext)
}

pub fn encryption_key_for_user(username: &str) -> [u8; 16] {
    let mut entropy = ENTROPY;
    for (idx, byte) in username.as_bytes().iter().take(entropy.len()).enumerate() {
        entropy[idx] ^= *byte;
    }

    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(&entropy, SALT, ITERATIONS, &mut key);
    key
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_token_from_query_or_text() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789";
        assert_eq!(
            extract_token_from_uri(&format!("wtcg://login?ST={token}&foo=bar")).unwrap(),
            token
        );
        assert_eq!(
            extract_token_from_uri(&format!(
                "http://127.0.0.1:12345/callback?ST={token}&foo=bar"
            ))
            .unwrap(),
            token
        );
        assert_eq!(
            extract_token_from_uri(&format!("copy this {token} please")).unwrap(),
            token
        );
    }

    #[test]
    fn encrypts_to_game_expected_length() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789";
        let encrypted = encrypt_token_for_user(token, "sgct").unwrap();
        assert_eq!(encrypted.len(), TOKEN_CIPHERTEXT_LEN);
    }

    #[test]
    fn accepts_short_eight_digit_account_id() {
        // Regression test: older accounts can have an 8-digit id instead of
        // 9, which used to fail the old `len() == 45` check entirely.
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-12345678";
        assert!(looks_like_token(token));
        assert_eq!(
            extract_token_from_uri(&format!("wtcg://login?ST={token}")).unwrap(),
            token
        );
        let encrypted = encrypt_token_for_user(token, "sgct").unwrap();
        assert_eq!(encrypted.len(), TOKEN_CIPHERTEXT_LEN);
    }

    #[test]
    fn does_not_truncate_a_longer_trailing_id_into_a_false_match() {
        // If the account id segment were, say, 13 digits (beyond our
        // accepted range), we should not silently accept the first 11 as a
        // "token" and leave 2 stray digits dangling.
        let too_long = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567890123";
        assert!(!looks_like_token(too_long));
    }
}
