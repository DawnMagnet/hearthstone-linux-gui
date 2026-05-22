#![allow(non_snake_case)]

use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn T5_Initialize(
    _bundle_identifier: *const c_char,
    _logger_name: *const c_char,
    _log_callback: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn T5_Dispose() {}

#[no_mangle]
pub extern "C" fn T5_GetAppMemoryFootprintBytes() -> u64 {
    0
}

#[no_mangle]
pub extern "C" fn T5_GetAppMemoryResidentBytes() -> u64 {
    0
}

#[no_mangle]
pub extern "C" fn T5_GetSystemMemoryAvailableBytes() -> u64 {
    0
}

#[no_mangle]
pub extern "C" fn T5_GetDiskSpaceAvailableBytes(_path: *const c_char) -> u64 {
    0
}

#[no_mangle]
pub extern "C" fn T5_GetDiskSpaceUsedBytes(_path: *const c_char) -> u64 {
    0
}

#[no_mangle]
pub extern "C" fn T5_GetDeviceThermalState() -> i32 {
    0
}
