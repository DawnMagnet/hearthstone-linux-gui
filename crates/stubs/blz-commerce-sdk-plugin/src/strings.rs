//! Strings crossing the SWIG boundary.
//!
//! Outgoing strings go through `SWIGRegisterStringCallback_battlenet_commerce`
//! when the game has registered it. The callback copies the bytes during the
//! call. The pointer it returns is what the P/Invoke marshaller consumes, so
//! this side does not free it. With no callback, a static string is returned
//! as-is and a dynamic string is leaked, which matches the previous stub and
//! keeps the pointer valid for the caller.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLBACK: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn register(callback: usize) {
    CALLBACK.store(callback, Ordering::SeqCst);
}

pub(crate) fn lossy(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: callers pass null or a NUL-terminated string owned by the game
    // or by a CString that outlives this read. The copy is owned from here on.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn text_from_ptr(ptr: *const c_char) -> crate::registry::Text {
    if ptr.is_null() {
        crate::registry::Text::Null
    } else {
        crate::registry::Text::Value(lossy(ptr))
    }
}

pub(crate) fn export(text: &str) -> *const c_char {
    let owned = CString::new(text.replace('\0', "")).unwrap_or_default();
    let callback = CALLBACK.load(Ordering::SeqCst);
    if callback == 0 {
        return owned.into_raw();
    }
    // `owned` stays alive until the callback returns.
    invoke_string_callback(callback, owned.as_ptr())
}

pub(crate) fn export_static(text: &'static CStr) -> *const c_char {
    let callback = CALLBACK.load(Ordering::SeqCst);
    if callback == 0 {
        return text.as_ptr();
    }
    invoke_string_callback(callback, text.as_ptr())
}

pub(crate) fn export_text(text: crate::registry::Text) -> *const c_char {
    match text {
        crate::registry::Text::Unset => export(""),
        crate::registry::Text::Null => std::ptr::null(),
        crate::registry::Text::Value(value) => export(&value),
    }
}

fn invoke_string_callback(callback: usize, text: *const c_char) -> *const c_char {
    // SAFETY: `callback` is the function pointer the game registered. It copies
    // `text` before it returns. The returned pointer belongs to the marshaller.
    let function: extern "C" fn(*const c_char) -> *mut c_char =
        unsafe { std::mem::transmute(callback) };
    function(text) as *const c_char
}

pub(crate) fn invoke_listener(callback: usize, owner: isize, event: isize) {
    // SAFETY: `callback` is the listener passed to blz_commerce_add_listener.
    // Linux Mono uses the C ABI. The registry lock is not held, so the listener
    // may re-enter the SDK.
    let function: extern "C" fn(isize, isize) = unsafe { std::mem::transmute(callback) };
    function(owner, event);
}

#[cfg(test)]
pub(crate) fn reset() {
    CALLBACK.store(0, Ordering::SeqCst);
}
