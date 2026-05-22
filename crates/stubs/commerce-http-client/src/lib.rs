#![allow(non_snake_case)]

use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn SetHttpOptions(
    _max_connections: i32,
    _request_timeout_seconds: i32,
    _persistent_data_path: *const c_char,
    _disable_server_verify: bool,
    _disable_certificate_revoke_check: bool,
) -> isize {
    1
}

#[no_mangle]
pub extern "C" fn CreateHttpClient() -> isize {
    1
}

#[no_mangle]
pub extern "C" fn CreateHttpClientWithRootCAs(_root_cas: *const c_char) -> isize {
    1
}

#[no_mangle]
pub extern "C" fn DestroyHttpClient(_client: isize) {}

#[no_mangle]
pub extern "C" fn AddHttpObserver(_client: isize, _observer: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn RemoveHttpObserver(_client: isize, _observer: isize) -> i32 {
    0
}
