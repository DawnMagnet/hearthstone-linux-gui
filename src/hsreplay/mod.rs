//! Uploads the player's collection to HSReplay.net, like Hearthstone Deck
//! Tracker does on Windows.
//!
//! The collection is read from the game's memory (see [`mono`]) once the main
//! menu has loaded it, then sent with HSReplay's collection API. Login is
//! OAuth2 with PKCE through the browser; the token is kept in the launcher's
//! config directory.

pub mod mono;

use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;

/// HDT's public OAuth client; its redirect URIs are localhost:17781-17789.
// ponytail: borrowed client id; register one for this launcher with HSReplay if they ask.
const CLIENT_ID: &str = "jIpNwuUWLFI6S3oeQkO3xlW6UCnfogw1IpAbFXqq";
const CALLBACK_PORTS: std::ops::Range<u16> = 17781..17790;
const AUTHORIZE_URL: &str = "https://hsreplay.net/oauth2/authorize/";
const TOKEN_URL: &str = "https://hsreplay.net/oauth2/token/";
const API: &str = "https://hsreplay.net/api/v1";
const USER_AGENT: &str = concat!("hearthstone-linux-gui/", env!("CARGO_PKG_VERSION"));

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// The collection shows up about 20 s after the game starts; leave room for
/// slow disks and the login queue.
const COLLECTION_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Waits for the game started as `launched_pid` (or one of its descendants)
/// to load the collection, then uploads it. Logs in first when no token is
/// saved, opening the login page with `open_url`. Blocks; run it on its own
/// thread. Returns the uploaded card count and dust.
pub fn sync_after_launch(
    launched_pid: u32,
    token_file: &Path,
    open_url: impl Fn(&str),
) -> Result<(i64, u64)> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()?;
    let token = runtime.block_on(token(&client, token_file, open_url))?;

    let state = wait_for_collection(launched_pid)?;
    runtime.block_on(upload(&client, &token, &state))?;
    let (cards, dust) = (state.owned_copies(), state.dust.unwrap_or(0));
    tracing::info!(cards, dust, "collection uploaded to HSReplay");
    Ok((cards, dust))
}

fn wait_for_collection(launched_pid: u32) -> Result<mono::GameState> {
    let deadline = Instant::now() + COLLECTION_TIMEOUT;
    let mut last_error = None;
    while Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
        let Some(pid) = mono::find_game(launched_pid) else {
            if !Path::new(&format!("/proc/{launched_pid}")).exists() {
                bail!("the game exited before its collection was loaded");
            }
            continue;
        };
        match mono::read_state(pid) {
            Ok(state) if state.owned_copies() > 0 => return Ok(state),
            Ok(_) => tracing::debug!("collection still empty"),
            Err(error) => {
                tracing::debug!(error = %format!("{error:#}"), "collection not readable yet");
                last_error = Some(error);
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("no collection"))
        .context("timed out waiting for the collection"))
}

/// HDT's collection JSON: dbfId -> [normal, golden, diamond, signature, then
/// the same four for trial copies].
fn collection_json(state: &mono::GameState) -> Value {
    let mut collection: BTreeMap<i32, [i32; 8]> = BTreeMap::new();
    for card in &state.cards {
        let Ok(premium @ 0..=3) = usize::try_from(card.premium) else {
            continue;
        };
        let row = collection.entry(card.dbf_id).or_default();
        row[premium] = card.count;
        row[4 + premium] = card.trial;
    }
    let collection: BTreeMap<String, _> = collection
        .into_iter()
        .map(|(dbf_id, row)| (dbf_id.to_string(), row))
        .collect();
    json!({
        "collection": collection,
        "favorite_heroes": {},
        "cardbacks": [],
        "favorite_cardback": 0,
        "dust": state.dust.unwrap_or(0),
        "player_records": {},
    })
}

async fn upload(client: &reqwest::Client, token: &Token, state: &mono::GameState) -> Result<()> {
    let auth = format!("{} {}", token.token_type, token.access_token);
    let (hi, lo) = (state.account_hi, state.account_lo);

    // Links the Blizzard account to the HSReplay account; harmless when it
    // already is.
    if let Some(battletag) = &state.battletag {
        let response = client
            .post(format!("{API}/blizzard_accounts/{hi}/{lo}/"))
            .header("Authorization", &auth)
            .json(&json!({ "battletag": battletag }))
            .send()
            .await?;
        tracing::debug!(status = %response.status(), "claimed Blizzard account");
    }

    let upload: Value = client
        .get(format!("{API}/collection/upload_request/"))
        .query(&[
            ("account_hi", hi.to_string()),
            ("account_lo", lo.to_string()),
            ("type", "CONSTRUCTED".to_string()),
        ])
        .header("Authorization", &auth)
        .send()
        .await?
        .error_for_status()
        .context("HSReplay refused the upload request")?
        .json()
        .await?;
    let url = upload["url"]
        .as_str()
        .context("upload request without url")?;
    let method = upload["method"].as_str().unwrap_or("PUT");
    let content_type = upload["content_type"]
        .as_str()
        .unwrap_or("application/json");

    // Gzipped like HDT does; the presigned URL may sign these headers.
    let mut body = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    body.write_all(&serde_json::to_vec(&collection_json(state))?)?;
    client
        .request(method.parse()?, url)
        .header("Content-Type", content_type)
        .header("Content-Encoding", "gzip")
        .body(body.finish()?)
        .send()
        .await?
        .error_for_status()
        .context("collection upload failed")?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct Token {
    access_token: String,
    refresh_token: String,
    #[serde(default = "bearer")]
    token_type: String,
    #[serde(default)]
    expires_in: u64,
    #[serde(default)]
    obtained_at: u64,
}

fn bearer() -> String {
    "Bearer".into()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// A valid access token: the saved one, refreshed if needed, or a new login.
async fn token(client: &reqwest::Client, file: &Path, open_url: impl Fn(&str)) -> Result<Token> {
    let saved = fs::read(file)
        .ok()
        .and_then(|data| serde_json::from_slice::<Token>(&data).ok());
    let token = match saved {
        Some(token) if now() + 60 < token.obtained_at + token.expires_in => return Ok(token),
        Some(token) => {
            let form = [
                ("grant_type", "refresh_token"),
                ("refresh_token", &token.refresh_token),
                ("client_id", CLIENT_ID),
            ];
            match request_token(client, &form).await {
                Ok(token) => token,
                Err(error) => {
                    tracing::info!(error = %format!("{error:#}"), "token refresh failed; logging in again");
                    login(client, open_url).await?
                }
            }
        }
        None => login(client, open_url).await?,
    };
    save_token(file, &token)?;
    Ok(token)
}

async fn request_token(client: &reqwest::Client, form: &[(&str, &str)]) -> Result<Token> {
    let mut token: Token = client
        .post(TOKEN_URL)
        .form(form)
        .send()
        .await?
        .error_for_status()
        .context("HSReplay token request failed")?
        .json()
        .await?;
    token.obtained_at = now();
    Ok(token)
}

fn save_token(file: &Path, token: &Token) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .with_context(|| format!("failed to write {}", file.display()))?;
    out.write_all(&serde_json::to_vec(token)?)?;
    Ok(())
}

/// OAuth2 authorization code flow with PKCE and a one-shot local callback server.
async fn login(client: &reqwest::Client, open_url: impl Fn(&str)) -> Result<Token> {
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = hex::encode(random_bytes::<16>()?);

    let listener = CALLBACK_PORTS
        .clone()
        .find_map(|port| TcpListener::bind(("127.0.0.1", port)).ok())
        .context("no free port for the HSReplay login callback (17781-17789)")?;
    let redirect_uri = format!("http://localhost:{}/", listener.local_addr()?.port());
    let authorize = Url::parse_with_params(
        AUTHORIZE_URL,
        [
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", &redirect_uri),
            ("scope", "fullaccess"),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ],
    )?;
    tracing::info!("opening HSReplay login page");
    open_url(authorize.as_str());

    let code = wait_for_callback(&listener, &state)?;
    drop(listener);
    let form = [
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("client_id", CLIENT_ID),
        ("redirect_uri", &redirect_uri),
        ("code_verifier", &verifier),
    ];
    request_token(client, &form).await
}

fn wait_for_callback(listener: &TcpListener, state: &str) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        ensure!(Instant::now() < deadline, "HSReplay login timed out");
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut request = [0; 4096];
        let len = stream.read(&mut request).unwrap_or(0);
        let request = String::from_utf8_lossy(&request[..len]);
        // "GET /?code=...&state=... HTTP/1.1"
        let Some(path) = request.split_whitespace().nth(1) else {
            continue;
        };
        let query: BTreeMap<_, _> = Url::parse(&format!("http://localhost{path}"))?
            .query_pairs()
            .into_owned()
            .collect();
        if !query.contains_key("code") && !query.contains_key("error") {
            continue; // favicon and the like
        }
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
              <h2>HSReplay login done. You can close this tab.</h2>",
        );
        ensure!(
            query.get("state").map(String::as_str) == Some(state),
            "HSReplay login failed: {}",
            query.get("error").map_or("state mismatch", String::as_str)
        );
        return query
            .get("code")
            .cloned()
            .context("HSReplay login returned no code");
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mono::{Card, GameState};

    #[test]
    fn builds_hdt_collection_rows() {
        let card = |dbf_id, premium, count, trial| Card {
            dbf_id,
            premium,
            count,
            trial,
        };
        let state = GameState {
            account_hi: 1,
            account_lo: 2,
            battletag: None,
            dust: Some(2280),
            cards: vec![
                card(42, 0, 2, 0),
                card(42, 1, 1, 0),
                card(7, 3, 1, 1),
                card(9, 5, 1, 0), // unknown finish: ignored
            ],
        };
        let json = collection_json(&state);
        assert_eq!(json["collection"]["42"], json!([2, 1, 0, 0, 0, 0, 0, 0]));
        assert_eq!(json["collection"]["7"], json!([0, 0, 0, 1, 0, 0, 0, 1]));
        assert!(json["collection"].get("9").is_none());
        assert_eq!(json["dust"], 2280);
    }
}
