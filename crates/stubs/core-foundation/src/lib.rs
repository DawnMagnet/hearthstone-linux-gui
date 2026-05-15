#![allow(non_snake_case)]

use std::{fs::File, io::Read, os::raw::c_char, ptr};

const KEY_LENGTH: usize = 0x30;

#[no_mangle]
pub extern "C" fn CFPreferencesAppSynchronize(_application_id: isize) -> bool {
    true
}

#[no_mangle]
pub extern "C" fn CFPreferencesCopyAppValue(_key: isize, _application_id: isize) -> isize {
    1
}

#[no_mangle]
pub extern "C" fn __CFStringMakeConstantString(_str: *mut c_char) -> isize {
    0
}

#[no_mangle]
pub extern "C" fn CFDataGetBytePtr(_data: isize) -> *mut u8 {
    let mut buffer = vec![0; KEY_LENGTH];
    let Ok(mut file) = File::open("token") else {
        return ptr::null_mut();
    };
    if file.read_exact(&mut buffer).is_err() {
        return ptr::null_mut();
    }

    Box::leak(buffer.into_boxed_slice()).as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn CFDataGetLength(_data: isize) -> u32 {
    KEY_LENGTH as u32
}

#[no_mangle]
pub extern "C" fn CFDataGetTypeID() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CFStringGetTypeID() -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CFGetTypeID(_cf: isize) -> i32 {
    0
}
