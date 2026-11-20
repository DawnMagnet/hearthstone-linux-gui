//! Options holder for `commerce_http_client`.
//!
//! The game creates a client and passes the integer handle to the commerce
//! SDK. HTTP paths, methods, headers, and token exchange are not specified, so
//! this library records the options and does not open a socket. The handle is
//! not a shared object with `blz_commerce_sdk_plugin`; each library has its
//! own copy of process memory.

#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Options {
    max_connections: i32,
    timeout_seconds: i32,
    ca_bundle: String,
    disable_server_verify: bool,
    disable_revoke_check: bool,
}

struct Client {
    // Read by tests. The game does not query these back, and nothing here
    // opens a connection with them.
    #[cfg_attr(not(test), allow(dead_code))]
    options: Options,
    #[cfg_attr(not(test), allow(dead_code))]
    root_cas: String,
}

struct State {
    options: Options,
    next: i64,
    clients: HashMap<i64, Client>,
}

impl State {
    fn fresh() -> Self {
        Self {
            options: Options::default(),
            next: 0,
            clients: HashMap::new(),
        }
    }
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::fresh()))
}

fn with_state<R>(body: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
    body(&mut guard)
}

fn lossy(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: the game passes null or a NUL-terminated string. The copy is owned.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[no_mangle]
pub extern "C" fn SetHttpOptions(
    max_connections: i32,
    request_timeout_seconds: i32,
    ca_bundle: *const c_char,
    disable_server_verify: bool,
    disable_certificate_revoke_check: bool,
) -> isize {
    with_state(|state| {
        state.options = Options {
            max_connections,
            timeout_seconds: request_timeout_seconds,
            ca_bundle: lossy(ca_bundle),
            disable_server_verify,
            disable_revoke_check: disable_certificate_revoke_check,
        };
    });
    1
}

#[no_mangle]
pub extern "C" fn CreateHttpClient() -> isize {
    create_client(String::new())
}

#[no_mangle]
pub extern "C" fn CreateHttpClientWithRootCAs(root_cas: *const c_char) -> isize {
    create_client(lossy(root_cas))
}

fn create_client(root_cas: String) -> isize {
    with_state(|state| {
        let Some(id) = state.next.checked_add(1) else {
            return 0;
        };
        state.next = id;
        state.clients.insert(
            id,
            Client {
                options: state.options.clone(),
                root_cas,
            },
        );
        isize::try_from(id).unwrap_or(0)
    })
}

#[no_mangle]
pub extern "C" fn DestroyHttpClient(client: isize) {
    let Ok(id) = i64::try_from(client) else {
        return;
    };
    if id == 0 {
        return;
    }
    with_state(|state| {
        state.clients.remove(&id);
    });
}

#[no_mangle]
pub extern "C" fn AddHttpObserver(_client: isize, _observer: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn RemoveHttpObserver(_client: isize, _observer: isize) -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::{
        state, AddHttpObserver, CreateHttpClient, CreateHttpClientWithRootCAs, DestroyHttpClient,
        RemoveHttpObserver, SetHttpOptions, State,
    };
    use std::ffi::CString;
    use std::sync::{Mutex, MutexGuard};

    static HARNESS: Mutex<()> = Mutex::new(());

    fn harness() -> MutexGuard<'static, ()> {
        let guard = HARNESS.lock().unwrap_or_else(|error| error.into_inner());
        *state().lock().unwrap_or_else(|error| error.into_inner()) = State::fresh();
        guard
    }

    fn socket_rows() -> usize {
        [
            "/proc/self/net/tcp",
            "/proc/self/net/tcp6",
            "/proc/self/net/udp",
            "/proc/self/net/udp6",
        ]
        .into_iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .map(|text| text.lines().count().saturating_sub(1))
                .unwrap_or(0)
        })
        .sum()
    }

    #[test]
    fn options_are_stored_and_no_socket_is_opened() {
        let _guard = harness();
        assert!(std::path::Path::new("/proc/self/net/tcp").exists());
        let before = socket_rows();
        let bundle = CString::new("").unwrap();
        assert_eq!(SetHttpOptions(2, 60, bundle.as_ptr(), false, false), 1);
        let first = CreateHttpClient();
        assert_eq!(first, 1);
        let roots = CString::new("local-root").unwrap();
        let second = CreateHttpClientWithRootCAs(roots.as_ptr());
        assert_eq!(second, 2);
        assert_eq!(AddHttpObserver(first, 0), 0);
        assert_eq!(RemoveHttpObserver(first, 0), 0);

        // A later options change does not rewrite a client that already exists,
        // and it still does not open a connection.
        let other = CString::new("bundle").unwrap();
        SetHttpOptions(4, 10, other.as_ptr(), true, true);
        {
            let guard = state().lock().unwrap_or_else(|error| error.into_inner());
            let first_client = guard.clients.get(&1).expect("first client");
            assert_eq!(first_client.options.max_connections, 2);
            assert_eq!(first_client.options.timeout_seconds, 60);
            assert!(!first_client.options.disable_server_verify);
            assert!(first_client.root_cas.is_empty());
            let second_client = guard.clients.get(&2).expect("second client");
            assert_eq!(second_client.root_cas, "local-root");
            assert_eq!(second_client.options.max_connections, 2);
        }

        DestroyHttpClient(first);
        DestroyHttpClient(first);
        DestroyHttpClient(0);
        let guard = state().lock().unwrap_or_else(|error| error.into_inner());
        assert!(!guard.clients.contains_key(&1));
        assert!(guard.clients.contains_key(&2));
        drop(guard);
        assert_eq!(socket_rows(), before);
    }
}
