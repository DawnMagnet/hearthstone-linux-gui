use crate::{paths::AppPaths, AppConfig, Region};
use aes::Aes128;
use anyhow::{Context, Result};
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use url::Url;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;

const ENTROPY: [u8; 16] = [
    200, 118, 244, 174, 76, 149, 46, 254, 242, 250, 15, 84, 25, 192, 156, 67,
];
const SALT: &[u8] = b"someSalt";
const ITERATIONS: u32 = 1000;
const TOKEN_CIPHERTEXT_LEN: usize = 0x30;
const LOCAL_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub struct LocalCallbackServer {
    pub login_url: String,
    pub cancel: Arc<AtomicBool>,
    pub receiver: mpsc::Receiver<Result<()>>,
}

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
    let bytes = value.as_bytes();
    bytes.len() == 45
        && bytes[2] == b'-'
        && bytes[35] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| idx == 2 || idx == 35 || byte.is_ascii_alphanumeric())
}

fn find_token_candidate(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if bytes.len() < 45 {
        return None;
    }

    for start in 0..=(bytes.len() - 45) {
        let candidate = &input[start..start + 45];
        if looks_like_token(candidate) {
            return Some(candidate.to_string());
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
    write_encrypted_token_for_current_user(&game_dir.join("token"), &token)?;
    config.game_dir = Some(game_dir);
    config.logged_in = true;
    config.last_login_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string(),
    );
    config.save(&paths.config_file)
}

pub fn start_local_callback_server(paths: AppPaths, region: Region) -> Result<LocalCallbackServer> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("failed to bind login callback")?;
    listener
        .set_nonblocking(true)
        .context("failed to configure login callback listener")?;
    let callback_url = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );
    let login_url = region.login_url_with_callback(&callback_url);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let deadline = Instant::now() + LOCAL_CALLBACK_TIMEOUT;
        loop {
            if cancel_for_thread.load(Ordering::Relaxed) {
                return;
            }
            if Instant::now() >= deadline {
                let _ = sender.send(Err(anyhow::anyhow!("login timed out")));
                return;
            }

            match listener.accept() {
                Ok((stream, _)) => match handle_http_callback(&paths, stream) {
                    CallbackOutcome::Complete(result) => {
                        let _ = sender.send(result);
                        return;
                    }
                    CallbackOutcome::Continue => {}
                },
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    let _ = sender.send(Err(error).context("failed to accept login callback"));
                    return;
                }
            }
        }
    });

    Ok(LocalCallbackServer {
        login_url,
        cancel,
        receiver,
    })
}

enum CallbackOutcome {
    Complete(Result<()>),
    Continue,
}

fn handle_http_callback(paths: &AppPaths, mut stream: TcpStream) -> CallbackOutcome {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0u8; 8192];
    let read = match stream.read(&mut buffer) {
        Ok(0) => return CallbackOutcome::Continue,
        Ok(read) => read,
        Err(_) => return CallbackOutcome::Continue,
    };

    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(target) = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return CallbackOutcome::Continue;
    };

    let uri = if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("http://127.0.0.1{target}")
    };

    match handle_callback_uri(paths, &uri) {
        Ok(()) => {
            let _ = write_http_response(
                &mut stream,
                "200 OK",
                "Login complete. You can return to hearthstone-linux-gui.",
            );
            CallbackOutcome::Complete(Ok(()))
        }
        Err(error) => {
            if callback_contains_error(&uri) {
                let _ = write_http_response(
                    &mut stream,
                    "400 Bad Request",
                    "Battle.net did not return a Hearthstone login token.",
                );
                CallbackOutcome::Complete(Err(error))
            } else {
                let _ = write_http_response(
                    &mut stream,
                    "202 Accepted",
                    "Waiting for Battle.net login to finish.",
                );
                CallbackOutcome::Continue
            }
        }
    }
}

fn callback_contains_error(uri: &str) -> bool {
    Url::parse(uri).is_ok_and(|url| {
        url.query_pairs()
            .any(|(key, _)| key.eq_ignore_ascii_case("error"))
    })
}

fn write_http_response(stream: &mut TcpStream, status: &str, message: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>hearthstone-linux-gui</title><body><p>{message}</p></body>"
    );
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
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
}
