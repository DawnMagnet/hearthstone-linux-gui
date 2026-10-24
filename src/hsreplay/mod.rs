//! Uploads the player's collection to HSReplay.net, like Hearthstone Deck
//! Tracker does on Windows.
//!
//! The collection is read from the game's memory (see [`mono`]) once the main
//! menu has loaded it, then sent with HSReplay's collection API. Login is
//! OAuth2 with PKCE through the browser; the token is kept in the launcher's
//! config directory.
//!
//! What is sent to HSReplay.net: the Blizzard account id (hi/lo) and BattleTag
//! (to link the account), the card collection and, when it could be read
//! unambiguously, the arcane dust balance. Nothing else is read or sent.

pub mod mono;

use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    future::Future,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
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

/// Only one sync (and so one login page and callback listener) at a time.
static SYNC_RUNNING: AtomicBool = AtomicBool::new(false);

struct RunningGuard;

impl RunningGuard {
    fn acquire() -> Option<Self> {
        SYNC_RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            .then_some(Self)
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        SYNC_RUNNING.store(false, Ordering::Release);
    }
}

/// Raised when the sync was cancelled, for example because the game stopped.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HSReplay sync cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Whether `error` wraps [`Cancelled`], possibly below some context.
pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<Cancelled>().is_some())
}

/// What a finished sync uploaded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Synced {
    pub cards: i64,
    /// `None` when the dust balance could not be read unambiguously; it is
    /// then left out of the upload.
    pub dust: Option<u64>,
}

/// Waits for the game started as `launched_pid` (or one of its descendants)
/// to load the collection, then uploads it. Logs in first when no token is
/// saved, opening the login page with `open_url`. Blocks; run it on its own
/// thread.
///
/// Cancelling `cancel` ends the sync with [`Cancelled`], as does the game
/// exiting. `Ok(None)` means another sync is already running, so nothing was
/// started.
pub fn sync_after_launch(
    launched_pid: u32,
    token_file: &Path,
    cancel: &CancellationToken,
    open_url: impl Fn(&str),
) -> Result<Option<Synced>> {
    let Some(_running) = RunningGuard::acquire() else {
        tracing::info!("an HSReplay sync is already running; not starting another");
        return Ok(None);
    };
    // Also ends when the game does, so a login page nobody finishes does not
    // keep the sync (or the CLI) waiting.
    let cancel = cancel.child_token();
    let _watcher = GameWatcher::start(launched_pid, cancel.clone());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()?;
    let token = runtime.block_on(cancellable(
        &cancel,
        token(&client, token_file, &cancel, open_url),
    ))?;

    let state = wait_for_collection(launched_pid, &cancel)?;
    runtime.block_on(cancellable(&cancel, upload(&client, &token, &state)))?;
    let synced = Synced {
        cards: state.owned_copies(),
        dust: state.dust,
    };
    tracing::info!(cards = synced.cards, dust = ?synced.dust, "collection uploaded to HSReplay");
    Ok(Some(synced))
}

/// Cancels a token when the launched game process is gone.
struct GameWatcher(CancellationToken);

impl GameWatcher {
    fn start(pid: u32, cancel: CancellationToken) -> Self {
        let done = CancellationToken::new();
        let stop = done.clone();
        std::thread::spawn(move || {
            while !stop.is_cancelled() {
                if !mono::process_alive(pid) {
                    cancel.cancel();
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Self(done)
    }
}

impl Drop for GameWatcher {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Runs `future` unless `cancel` fires first.
async fn cancellable<T>(
    cancel: &CancellationToken,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(Cancelled.into()),
        result = future => result,
    }
}

/// Sleeps `duration` in short steps; false when `cancel` fired meanwhile.
fn sleep_unless_cancelled(duration: Duration, cancel: &CancellationToken) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200).min(duration));
    }
    !cancel.is_cancelled()
}

/// A state read from corrupt or half-initialised memory must not be uploaded.
fn plausible(state: &mono::GameState) -> bool {
    state.cards.len() < 100_000
        && state.cards.iter().all(|card| {
            (0..1_000_000).contains(&card.dbf_id)
                && (0..=1000).contains(&card.count)
                && (0..=1000).contains(&card.trial)
        })
        && state.dust.is_none_or(|dust| dust < 10_000_000)
}

fn wait_for_collection(launched_pid: u32, cancel: &CancellationToken) -> Result<mono::GameState> {
    let deadline = Instant::now() + COLLECTION_TIMEOUT;
    let mut last_error = None;
    while Instant::now() < deadline {
        if !sleep_unless_cancelled(POLL_INTERVAL, cancel) {
            return Err(Cancelled.into());
        }
        let Some(pid) = mono::find_game(launched_pid) else {
            if !mono::process_alive(launched_pid) {
                bail!("the game exited before its collection was loaded");
            }
            continue;
        };
        match mono::read_state(pid) {
            Ok(state) if state.owned_copies() > 0 => {
                if plausible(&state) {
                    return Ok(state);
                }
                tracing::warn!("collection read from the game looks corrupt; trying again");
                last_error = Some(anyhow::anyhow!(
                    "the collection read from the game was not plausible"
                ));
            }
            Ok(_) => tracing::debug!("collection still empty"),
            Err(error) if mono::is_ptrace_denied(&error) => {
                return Err(error.context("cannot read the game's collection"));
            }
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
    let mut body = json!({
        "collection": collection,
        "favorite_heroes": {},
        "cardbacks": [],
        "favorite_cardback": 0,
        "player_records": {},
    });
    // Never send a made-up balance: leave the field out when it is unknown.
    if let Some(dust) = state.dust {
        body["dust"] = dust.into();
    }
    body
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
    #[serde(default)]
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

/// Whether the token can still be used, leaving a minute of slack. Saturating
/// arithmetic keeps a corrupt saved file from wrapping around.
fn token_is_valid(token: &Token, at: u64) -> bool {
    at.saturating_add(60) < token.obtained_at.saturating_add(token.expires_in)
}

/// Tightens an existing token file even when no refresh is needed, so a file
/// left world-readable by an older build does not stay that way.
fn restrict_token_permissions(file: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(file)?;
    if metadata.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(file, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to make {} private", file.display()))?;
    }
    Ok(())
}

/// The token endpoint turned the request down (HTTP 400/401, e.g.
/// `invalid_grant`), as opposed to being unreachable or failing.
#[derive(Debug)]
struct AuthRejected(u16);

impl std::fmt::Display for AuthRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HSReplay rejected the token request (HTTP {})", self.0)
    }
}

impl std::error::Error for AuthRejected {}

/// A valid access token: the saved one, refreshed if needed, or a new login.
/// Only a token the server really rejects leads to a new login; a network
/// problem or a server error leaves the saved token alone and fails, so the
/// next launch tries again.
async fn token(
    client: &reqwest::Client,
    file: &Path,
    cancel: &CancellationToken,
    open_url: impl Fn(&str),
) -> Result<Token> {
    let saved = match fs::read(file) {
        Ok(data) => match serde_json::from_slice::<Token>(&data) {
            Ok(token) => Some(token),
            Err(error) => {
                tracing::warn!(error = %error, "saved HSReplay token is unreadable; logging in again");
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(error = %error, "could not read the saved HSReplay token; logging in again");
            None
        }
    };
    let token = match saved {
        Some(token) if token_is_valid(&token, now()) => {
            if let Err(error) = restrict_token_permissions(file) {
                tracing::warn!(error = %format!("{error:#}"), "could not make the saved HSReplay token private");
            }
            return Ok(token);
        }
        Some(token) => {
            let form = [
                ("grant_type", "refresh_token"),
                ("refresh_token", &token.refresh_token),
                ("client_id", CLIENT_ID),
            ];
            match request_token(client, &form).await {
                Ok(mut refreshed) => {
                    if refreshed.refresh_token.is_empty() {
                        refreshed.refresh_token = token.refresh_token;
                    }
                    refreshed
                }
                Err(error) if error.is::<AuthRejected>() => {
                    tracing::info!(error = %format!("{error:#}"), "saved HSReplay token was rejected; logging in again");
                    login(client, cancel, open_url).await?
                }
                Err(error) => {
                    return Err(error.context(
                        "could not refresh the HSReplay token; will try again next time",
                    ));
                }
            }
        }
        None => login(client, cancel, open_url).await?,
    };
    save_token(file, &token)?;
    Ok(token)
}

async fn request_token(client: &reqwest::Client, form: &[(&str, &str)]) -> Result<Token> {
    let response = client.post(TOKEN_URL).form(form).send().await?;
    let status = response.status();
    if matches!(status.as_u16(), 400 | 401) {
        return Err(AuthRejected(status.as_u16()).into());
    }
    let mut token: Token = response
        .error_for_status()
        .context("HSReplay token request failed")?
        .json()
        .await?;
    token.obtained_at = now();
    Ok(token)
}

/// Writes the token through a temporary file and a rename, so a crash cannot
/// leave a half-written token, and with mode 0600 even when the file already
/// existed with wider permissions.
fn save_token(file: &Path, token: &Token) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    if let Some(parent) = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    let _ = fs::remove_file(&temporary);
    let write = || -> Result<()> {
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        out.write_all(&serde_json::to_vec(token)?)?;
        out.sync_all()?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        fs::rename(&temporary, file)
            .with_context(|| format!("failed to replace {}", file.display()))
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

/// OAuth2 authorization code flow with PKCE and a one-shot local callback server.
async fn login(
    client: &reqwest::Client,
    cancel: &CancellationToken,
    open_url: impl Fn(&str),
) -> Result<Token> {
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
    let challenge = pkce_challenge(&verifier);
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

    let code = wait_for_callback(&listener, &state, cancel)?;
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

/// PKCE S256 code challenge (RFC 7636 section 4.2).
fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn respond(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{body}"
        )
        .as_bytes(),
    );
}

/// Waits for the browser to come back to the callback listener with the
/// authorization code. Requests with a wrong `state` (another page poking at
/// the port, or a stale tab) get a 400 and are ignored; the wait goes on.
fn wait_for_callback(
    listener: &TcpListener,
    state: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        ensure!(!cancel.is_cancelled(), Cancelled);
        ensure!(Instant::now() < deadline, "HSReplay login timed out");
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let request = read_request_line(&stream);
        // "GET /?code=...&state=... HTTP/1.1"
        let Some(path) = request.split_whitespace().nth(1) else {
            continue;
        };
        let Ok(url) = Url::parse(&format!("http://localhost{path}")) else {
            respond(&mut stream, "400 Bad Request", "Bad request.");
            continue;
        };
        let query: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        if !query.contains_key("code") && !query.contains_key("error") {
            respond(&mut stream, "404 Not Found", "Not found."); // favicon and the like
            continue;
        }
        if query.get("state").map(String::as_str) != Some(state) {
            tracing::warn!("ignoring an HSReplay login callback with the wrong state");
            respond(
                &mut stream,
                "400 Bad Request",
                "Login state mismatch. Please try again from the HSReplay login page.",
            );
            continue;
        }
        if let Some(error) = query.get("error") {
            respond(
                &mut stream,
                "200 OK",
                "<h2>HSReplay login was not completed. You can close this tab.</h2>",
            );
            bail!("HSReplay login failed: {error}");
        }
        respond(
            &mut stream,
            "200 OK",
            "<h2>HSReplay login done. You can close this tab.</h2>",
        );
        return query
            .get("code")
            .cloned()
            .context("HSReplay login returned no code");
    }
}

/// The request line of an HTTP request, after reading the headers too: the
/// request may arrive in several packets, and closing a socket with unread
/// data resets it, so the client could miss the response.
fn read_request_line(stream: &std::net::TcpStream) -> String {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(stream).take(16 * 1024);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return String::new();
    }
    let mut header = String::new();
    while matches!(reader.read_line(&mut header), Ok(len) if len > 0) && header.trim() != "" {
        header.clear();
    }
    request_line
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

    #[test]
    fn leaves_dust_out_when_unknown() {
        let state = GameState {
            account_hi: 1,
            account_lo: 2,
            battletag: None,
            dust: None,
            cards: vec![],
        };
        assert!(collection_json(&state).get("dust").is_none());
    }

    #[test]
    fn recognises_cancellation_through_context() {
        assert!(is_cancelled(
            &anyhow::Error::new(Cancelled).context("waiting for the upload")
        ));
        assert!(!is_cancelled(&anyhow::anyhow!("not cancelled")));
    }

    #[test]
    fn pkce_challenge_matches_rfc_7636_vector() {
        // RFC 7636, appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    /// Sends one raw HTTP request to the listener and returns the status line.
    fn get(port: u16, path: &str) -> String {
        use std::io::{BufRead, BufReader};
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        // Split in two packets, as a real client may do.
        stream.write_all(b"GET ").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        write!(stream, "{path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut status = String::new();
        BufReader::new(stream).read_line(&mut status).unwrap();
        status.trim().to_string()
    }

    #[test]
    fn callback_ignores_a_wrong_state_and_accepts_the_right_one() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = CancellationToken::new();
        let waiter = std::thread::spawn(move || wait_for_callback(&listener, "good", &cancel));

        assert!(get(port, "/?code=stolen&state=bad").contains("400"));
        assert!(get(port, "/?code=stolen").contains("400"));
        assert!(get(port, "/favicon.ico").contains("404"));
        assert!(!waiter.is_finished(), "a bad state must not end the login");
        assert!(get(port, "/?code=abc123&state=good").contains("200"));
        assert_eq!(waiter.join().unwrap().unwrap(), "abc123");
    }

    #[test]
    fn callback_reports_a_denied_login_and_honours_cancel() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = CancellationToken::new();
        let waiter = std::thread::spawn({
            let cancel = cancel.clone();
            move || wait_for_callback(&listener, "good", &cancel)
        });
        assert!(get(port, "/?error=access_denied&state=good").contains("200"));
        let error = waiter.join().unwrap().unwrap_err();
        assert!(format!("{error:#}").contains("access_denied"));

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        cancel.cancel();
        let error = wait_for_callback(&listener, "good", &cancel).unwrap_err();
        assert!(error.is::<Cancelled>());
    }

    #[test]
    fn token_validity_handles_boundaries_and_overflow() {
        let token = |obtained_at, expires_in| Token {
            access_token: "a".into(),
            refresh_token: "r".into(),
            token_type: "Bearer".into(),
            expires_in,
            obtained_at,
        };
        assert!(token_is_valid(&token(100, 3600), 100 + 3600 - 61));
        assert!(!token_is_valid(&token(100, 3600), 100 + 3600 - 60));
        assert!(!token_is_valid(&token(0, 0), 0));
        // A corrupt saved token must not wrap the addition.
        assert!(token_is_valid(&token(u64::MAX - 10, u64::MAX), 0));
    }

    #[test]
    fn makes_an_existing_token_file_private_without_saving_again() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("hsreplay-token-perm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("token.json");
        fs::write(&file, b"{}").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        restrict_token_permissions(&file).unwrap();
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_token_is_private_and_replaces_an_existing_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("hsreplay-token-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("nested").join("token.json");
        let token = |access: &str| Token {
            access_token: access.into(),
            refresh_token: "r".into(),
            token_type: "Bearer".into(),
            expires_in: 3600,
            obtained_at: 1,
        };
        save_token(&file, &token("one")).unwrap();
        // An older, world-readable file must end up private.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        save_token(&file, &token("two")).unwrap();
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let saved: Token = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(saved.access_token, "two");
        assert!(!file.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
