//! Commerce bridge: an opt-in replacement for the `blz_commerce_sdk_plugin`
//! and `commerce_http_client` stubs.
//!
//! Blizzard ships the commerce SDK for Windows and macOS only. This library
//! exports the same functions as the game's P/Invoke declarations and
//! forwards every call over a pipe to `commerce_helper.exe`, which runs under
//! Wine and hosts the player's official Windows DLLs (downloaded from
//! Blizzard's CDN like the rest of the game). Pointers returned by the DLLs
//! stay opaque handles on this side; strings are copied; the listener
//! callback comes back over the pipe and may call into the SDK again while it
//! runs.
//!
//! The same file is loaded as both libraries (the launcher installs one
//! file and a symlink), so there is a single helper process.
//!
//! Configuration (set by the launcher):
//! - `HEARTHSTONE_COMMERCE_BRIDGE_DIR`: directory with `commerce_helper.exe`
//!   and the Windows DLLs.
//! - `HEARTHSTONE_COMMERCE_WINE` (default `wine`) and
//!   `HEARTHSTONE_COMMERCE_WINEPREFIX` (optional).

#![allow(non_snake_case)]

use std::{
    ffi::{CStr, CString},
    io::{self, BufReader},
    os::raw::c_char,
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

mod protocol;
use protocol::{ArgKind, Message, Reply, RetKind};

/// The game's SWIG string callback: turns a C string into a managed string.
static STRING_CALLBACK: AtomicUsize = AtomicUsize::new(0);
/// The game's commerce event listener.
static LISTENER: AtomicUsize = AtomicUsize::new(0);
static CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);
static FAILED: AtomicBool = AtomicBool::new(false);

const SDK: u8 = 0;
const HTTP: u8 = 1;

struct Connection {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Connection {
    fn start() -> io::Result<Self> {
        let dir = std::env::var_os("HEARTHSTONE_COMMERCE_BRIDGE_DIR")
            .ok_or_else(|| io::Error::other("HEARTHSTONE_COMMERCE_BRIDGE_DIR is not set"))?;
        let dir = Path::new(&dir);
        let wine = std::env::var_os("HEARTHSTONE_COMMERCE_WINE").unwrap_or_else(|| "wine".into());
        let mut command = Command::new(wine);
        command
            .arg(dir.join("commerce_helper.exe"))
            .arg(windows_path(dir))
            .arg(windows_path(Path::new(ca_bundle())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            // The game's library path and preloads (bundled glibc, MangoHud)
            // must not leak into Wine.
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD")
            .env(
                "WINEDEBUG",
                std::env::var("WINEDEBUG").unwrap_or_else(|_| "-all".into()),
            );
        if let Some(prefix) = std::env::var_os("HEARTHSTONE_COMMERCE_WINEPREFIX") {
            command.env("WINEPREFIX", prefix);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().expect("piped stdin");
        let output = BufReader::new(child.stdout.take().expect("piped stdout"));
        eprintln!("[commerce-bridge] started helper in {}", dir.display());
        Ok(Self {
            _child: child,
            input,
            output,
        })
    }
}

fn windows_path(path: &Path) -> String {
    format!("Z:{}", path.display()).replace('/', "\\")
}

fn ca_bundle() -> &'static str {
    [
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem",
        "/etc/ssl/cert.pem",
    ]
    .into_iter()
    .find(|path| Path::new(path).exists())
    .unwrap_or("/etc/ssl/certs/ca-certificates.crt")
}

/// One argument as received from the game: a register value whose meaning
/// depends on its kind.
fn collect_arg(kind: ArgKind, value: u64) -> protocol::Arg {
    match kind {
        ArgKind::Int => protocol::Arg::Int(value),
        ArgKind::Str => protocol::Arg::Str(c_string(value)),
        ArgKind::Listener => {
            LISTENER.store(value as usize, Ordering::SeqCst);
            protocol::Arg::Listener
        }
        ArgKind::Log => protocol::Arg::Log,
        ArgKind::StringCallback => {
            STRING_CALLBACK.store(value as usize, Ordering::SeqCst);
            protocol::Arg::Int(0)
        }
    }
}

fn c_string(pointer: u64) -> Option<Vec<u8>> {
    (pointer != 0).then(|| {
        unsafe { CStr::from_ptr(pointer as *const c_char) }
            .to_bytes()
            .to_vec()
    })
}

/// Forwards one call to the helper and returns its result as the game expects it.
fn call(library: u8, name: &str, kinds: &[ArgKind], values: &[u64], ret: RetKind) -> u64 {
    if kinds.contains(&ArgKind::StringCallback) {
        // Kept on this side: the helper registers its own.
        collect_arg(ArgKind::StringCallback, values[0]);
        return 0;
    }
    let args: Vec<_> = kinds
        .iter()
        .zip(values)
        .map(|(kind, value)| collect_arg(*kind, *value))
        .collect();
    match forward(library, name, &args, ret) {
        Ok(Reply::Int(value)) => value,
        Ok(Reply::Str(text)) => managed_string(text),
        Err(error) => {
            if !FAILED.swap(true, Ordering::SeqCst) {
                eprintln!("[commerce-bridge] {name} failed: {error}; the shop stays unavailable");
            }
            0
        }
    }
}

fn forward(library: u8, name: &str, args: &[protocol::Arg], ret: RetKind) -> io::Result<Reply> {
    if FAILED.load(Ordering::SeqCst) {
        return Err(io::Error::other("bridge is down"));
    }
    let lock = || {
        CONNECTION
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    };
    let closed = || io::Error::other("bridge closed");
    let mut guard = lock();
    if guard.is_none() {
        *guard = Some(Connection::start()?);
    }
    protocol::write_call(
        &mut guard.as_mut().ok_or_else(closed)?.input,
        library,
        name,
        args,
        ret,
    )?;
    loop {
        let message = protocol::read_message(&mut guard.as_mut().ok_or_else(closed)?.output, ret)?;
        match message {
            Message::Result(reply) => return Ok(reply),
            Message::Event { owner, event } => {
                // The game handles the event by calling back into the SDK, so
                // the connection must be free while the listener runs.
                drop(guard);
                notify_listener(owner, event);
                guard = lock();
                protocol::write_event_done(&mut guard.as_mut().ok_or_else(closed)?.input)?;
            }
        }
    }
}

fn notify_listener(owner: u64, event: u64) {
    let listener = LISTENER.load(Ordering::SeqCst);
    if listener != 0 {
        let listener: extern "C" fn(u64, u64) = unsafe { std::mem::transmute(listener) };
        listener(owner, event);
    }
}

/// Hands a returned string to the game the way SWIG does: through the
/// registered string callback, which returns memory the runtime frees.
fn managed_string(text: Option<Vec<u8>>) -> u64 {
    let Some(text) = text else { return 0 };
    let callback = STRING_CALLBACK.load(Ordering::SeqCst);
    if callback == 0 {
        return 0;
    }
    let text = CString::new(text).unwrap_or_default();
    let callback: extern "C" fn(*const c_char) -> u64 = unsafe { std::mem::transmute(callback) };
    callback(text.as_ptr())
}

macro_rules! forward {
    ($( $name:ident ( $( $arg:ident : $kind:ident ),* ) -> $ret:ident ; )*) => { $(
        #[no_mangle]
        pub extern "C" fn $name( $( $arg: u64 ),* ) -> u64 {
            call(SDK, stringify!($name), &[ $( ArgKind::$kind ),* ], &[ $( $arg ),* ], RetKind::$ret)
        }
    )* };
}

include!("sdk_table.rs");

// `commerce_http_client`, as used by HearthstoneCommerceHttpAdaptor.

#[no_mangle]
pub extern "C" fn SetHttpOptions(
    max_connections: u64,
    timeout_seconds: u64,
    certificate_bundle: u64,
    disable_server_verify: u64,
    disable_revoke_check: u64,
) {
    // The helper replaces the bundle path with the host's CA file.
    call(
        HTTP,
        "SetHttpOptions",
        &[
            ArgKind::Int,
            ArgKind::Int,
            ArgKind::Str,
            ArgKind::Int,
            ArgKind::Int,
        ],
        &[
            max_connections,
            timeout_seconds,
            certificate_bundle,
            disable_server_verify,
            disable_revoke_check,
        ],
        RetKind::Int,
    );
}

#[no_mangle]
pub extern "C" fn CreateHttpClient() -> u64 {
    call(HTTP, "CreateHttpClient", &[], &[], RetKind::Int)
}

#[no_mangle]
pub extern "C" fn DestroyHttpClient() {
    call(HTTP, "DestroyHttpClient", &[], &[], RetKind::Int);
}
