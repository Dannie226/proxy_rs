use std::{collections::HashMap, ffi::*, fmt::Debug};

use bstr::BString;

use crate::{
    Buffer, ConstBuffer,
    bio::{Reader, http_bio_read, http_destroy_reader},
};

pub type HeaderMap = HashMap<BString, Vec<BString>>;

pub struct Request {
    pub(crate) method: String,
    pub(crate) uri: String,
    pub(crate) version: (u32, u32),
    pub(crate) headers: HeaderMap,
    pub(crate) body: *mut Reader,
}

impl Request {
    /// SAFETY:
    /// body must have been created with a new reader function
    pub(crate) unsafe fn new(
        method: String,
        uri: String,
        version: (u32, u32),
        headers: HeaderMap,
        body: *mut Reader,
    ) -> *mut Request {
        let req = Request {
            method,
            uri,
            version,
            headers,
            body,
        };

        let req = Box::new(req);

        Box::into_raw(req)
    }

    /// SAFETY:
    /// Request must have been made with new
    /// Request must not be used after the call
    pub(crate) unsafe fn delete(req: *mut Request) {
        // SAFETY: Request was made with new, which gets the pointer through
        // Box::into_raw
        let r = unsafe { Box::from_raw(req) };

        // SAFETY: Request body reader was made with a new reader
        // function
        unsafe {
            http_destroy_reader(r.body);
        }
    }
}

impl Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Request {{
\tmethod: {:?}
\turi: {:?}
\tversion: {:?}
\theaders: {:?}
        }}",
            self.method, self.uri, self.version, self.headers
        )
    }
}

/// Caller must ensure the following
/// 1) Request must be convertable to a reference
/// 2) data must be convertable to a slice of len bytes
/// 3) res must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_read_body(
    request: *mut Request,
    data: *mut u8,
    len: usize,
    res: *mut crate::result::Result,
) {
    // SAFETY: Request is convertable to a reference
    let req = unsafe { request.as_mut_unchecked() };

    // SAFETY:
    // data is convertable to a slice of len bytes
    // read is convertable to a reference
    unsafe {
        http_bio_read(req.body, data.cast(), len, res);
    }
}

/// Caller must ensure the following
/// 1) Request must be convertable to a reference
/// 2) string must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_method(request: *const Request, string: *mut Buffer) -> c_int {
    // SAFETY: Request is convertable to a reference
    let req = unsafe { request.as_ref_unchecked() };

    // SAFETY: string is convertable to a reference
    let string = unsafe { string.as_mut_unchecked() };

    string.copy_slice(req.method.as_bytes())
}

/// Caller must ensure the following
/// 1) Request must be convertable to a reference
/// 2) string must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_uri(request: *const Request, string: *mut Buffer) -> c_int {
    // SAFETY: Request is convertable to a reference
    let req = unsafe { request.as_ref_unchecked() };

    // SAFETY: string is convertable to a reference
    let string = unsafe { string.as_mut_unchecked() };

    string.copy_slice(req.uri.as_bytes())
}

/// Caller must ensure the following
/// 1) Request must be convertable to a reference
/// 2) Major must be convertable to a reference, or be null
/// 3) Minor must be convertable to a reference, or be null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_version(
    request: *const Request,
    major: *mut u32,
    minor: *mut u32,
) {
    // SAFETY: Request is a valid, non-null pointer
    let req = unsafe { request.as_ref_unchecked() };

    // SAFETY: Major is either null, or convertable to a reference
    let major = unsafe { major.as_mut() };

    // SAFETY: Minor is either null, or convertable to a reference
    let minor = unsafe { minor.as_mut() };

    if let Some(m) = major {
        *m = req.version.0;
    }

    if let Some(m) = minor {
        *m = req.version.1;
    }
}

/// Caller must ensure the following:
/// 1) Request must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_header_count(
    request: *const Request,
    name: ConstBuffer,
) -> usize {
    // SAFETY: Request is a valid, non-null pointer
    let req = unsafe { request.as_ref_unchecked() };

    let header = name.as_slice();

    req.headers.get(header).map(Vec::len).unwrap_or(0)
}

/// Caller must ensure the following:
/// 1) Request must be convertable to a reference
/// 2) Value must be convertable to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_header(
    request: *const Request,
    name: ConstBuffer,
    header_index: usize,
    value: *mut Buffer,
) -> c_int {
    // SAFETY: Request is convertable to a reference
    let req = unsafe { request.as_ref_unchecked() };

    // SAFETY: value is convertable to a reference
    let value = unsafe { value.as_mut_unchecked() };

    let name = name.as_slice();

    let Some(headers) = req.headers.get(name) else {
        return 3;
    };

    let Some(header) = headers.get(header_index) else {
        return 4;
    };

    value.copy_slice(header)
}

/// SAFETY:
/// Request must have been made with new
/// Request must not be used after the call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_request(request: *mut Request) {
    unsafe {
        Request::delete(request);
    }
}
