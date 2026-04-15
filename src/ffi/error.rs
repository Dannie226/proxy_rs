#![allow(dead_code)]

use std::{
    ffi::{CStr, c_char, c_ulong},
    fmt::{Debug, Display},
    ptr,
};

pub struct Error(c_ulong);

impl Error {
    pub fn next_error() -> Error {
        let code = unsafe { ERR_get_error() };

        return Error(code);
    }

    pub fn get_error_code(&self) -> c_ulong {
        self.0
    }

    pub fn get_error_string(&self) -> &'static str {
        // SAFETY: The OpenSSL error string is all ASCII text
        unsafe {
            let buf = ERR_error_string(self.0, ptr::null_mut());
            str::from_utf8_unchecked(CStr::from_ptr(buf).to_bytes())
        }
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Open SSL Error ({}): {}",
            self.0,
            self.get_error_string()
        )
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_error_string())
    }
}

impl std::error::Error for Error {}

pub type Result<T> = ::core::result::Result<T, Error>;

unsafe extern "C" {
    fn ERR_get_error() -> c_ulong;
    fn ERR_error_string(e: c_ulong, buf: *mut c_char) -> *mut c_char;
}
