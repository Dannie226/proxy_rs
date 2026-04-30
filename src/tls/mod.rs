use openssl_sys::ERR_get_error;
use std::{
    ffi::*,
    fmt::{Debug, Display},
    ptr, str,
};

unsafe extern "C" {
    fn ERR_error_string(code: c_ulong, buf: *const c_char) -> *const c_char;
}

pub mod listener;
pub mod stream;

pub struct Error {
    code: c_ulong,
}

impl Error {
    fn next_error() -> Error {
        Error {
            // SAFETY: The function exists
            code: unsafe { ERR_get_error() },
        }
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: The function exists and can take NULL values, and always returns
        // a null terminated string
        // https://docs.openssl.org/master/man3/ERR_error_string
        let error_string = unsafe { CStr::from_ptr(ERR_error_string(self.code, ptr::null())) };

        // SAFETY: OpenSSL's error string is entirely ASCII text
        // https://docs.openssl.org/master/man3/ERR_error_string
        let error_string = unsafe { str::from_utf8_unchecked(error_string.to_bytes()) };

        write!(f, "OpenSSL Error ({}): {}", self.code, error_string)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: The function exists and can take NULL values, and always returns
        // a null terminated string
        // https://docs.openssl.org/master/man3/ERR_error_string
        let error_string = unsafe { CStr::from_ptr(ERR_error_string(self.code, ptr::null())) };

        // SAFETY: OpenSSL's error string is entirely ASCII text
        // https://docs.openssl.org/master/man3/ERR_error_string
        let error_string = unsafe { str::from_utf8_unchecked(error_string.to_bytes()) };

        write!(f, "{}", error_string)
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;
