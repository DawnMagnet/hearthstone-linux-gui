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
/// "XX-" + 32 payload characters + "-" + at least one account digit.
const TOKEN_MIN_LEN: usize = 37;
/// PKCS7 padding must keep the ciphertext at 0x30 bytes, so the plaintext
/// cannot exceed 47 characters.
const TOKEN_MAX_LEN: usize = 47;
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

pub fn looks_like_token(value: &str) -> bool {
    looks_like_token_bytes(value.as_bytes())
}

fn looks_like_token_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= TOKEN_MIN_LEN
        && bytes.len() <= TOKEN_MAX_LEN
        && is_token_prefix(bytes)
        && bytes[36..].iter().all(|byte| byte.is_ascii_digit())
}

/// Validates the fixed-width "XX-<32 characters>-" prefix of a token.
fn is_token_prefix(bytes: &[u8]) -> bool {
    bytes.len() >= 36
        && bytes[2] == b'-'
        && bytes[35] == b'-'
        && bytes[..2].iter().all(|byte| byte.is_ascii_alphanumeric())
        && bytes[3..35].iter().all(|byte| byte.is_ascii_alphanumeric())
}

fn find_token_candidate(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if bytes.len() < TOKEN_MIN_LEN {
        return None;
    }

    for start in 0..=(bytes.len() - TOKEN_MIN_LEN) {
        if !is_token_prefix(&bytes[start..start + 36]) {
            continue;
        }
        let mut end = start + 36;
        while end < bytes.len() && bytes[end].is_ascii_digit() && end - start < TOKEN_MAX_LEN {
            end += 1;
        }
        let candidate = &bytes[start..end];
        if looks_like_token_bytes(candidate) {
            return Some(String::from_utf8_lossy(candidate).into_owned());
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
    anyhow::ensure!(
        looks_like_token(token),
        "token format is invalid: expected two-character region, a dash, 32 alphanumeric \
         characters, a dash, and the numeric account id (for example \
         `US-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx-1234567`); got {} characters",
        token.len()
    );

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
    fn accepts_tokens_with_varying_account_id_lengths() {
        // 8-digit account id (44 characters, as reported in issue #10).
        assert!(looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-12345678"
        ));
        // 7-digit account id (43 characters).
        assert!(looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567"
        ));
        // 9-digit account id (45 characters, the previously hard-coded length).
        assert!(looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789"
        ));
        // 10-digit account id (46 characters).
        assert!(looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567890"
        ));
    }

    #[test]
    fn rejects_malformed_tokens() {
        // Missing the account id entirely.
        assert!(!looks_like_token("AB-0123456789ABCDEFGHIJKLMNOPQRSTUV"));
        // Missing the region prefix.
        assert!(!looks_like_token(
            "0123456789ABCDEFGHIJKLMNOPQRSTUV-12345678"
        ));
        // Dashes in the wrong places.
        assert!(!looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQR-UV-12345678"
        ));
        // Non-alphanumeric payload character.
        assert!(!looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTU!-12345678"
        ));
        // Non-digit account id.
        assert!(!looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567A"
        ));
        // Longer than the encrypted token file can hold.
        assert!(!looks_like_token(
            "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789012"
        ));
    }

    #[test]
    fn extracts_tokens_with_short_account_ids_from_text() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-12345678";
        assert_eq!(extract_token_from_uri(token).unwrap(), token);
        assert_eq!(
            extract_token_from_uri(&format!("http://localhost:0/?ST={token}")).unwrap(),
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
    fn encrypts_short_account_id_tokens_to_game_expected_length() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-12345678";
        let encrypted = encrypt_token_for_user(token, "sgct").unwrap();
        assert_eq!(encrypted.len(), TOKEN_CIPHERTEXT_LEN);
    }

    #[test]
    fn rejects_tokens_that_do_not_fit_the_encrypted_layout() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789012";
        assert!(encrypt_token_for_user(token, "sgct").is_err());
    }
}
