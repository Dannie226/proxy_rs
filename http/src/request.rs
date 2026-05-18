use std::{collections::HashMap, ffi::*, fmt::Debug, fmt::Write as _};

use bstr::BString;

use crate::{
    IsSane,
    bio::{Reader, http_bio_read, http_destroy_reader},
    buffer::{Buffer, ConstBuffer},
    function,
    result::HttpResult,
};

pub type HeaderMap = HashMap<BString, Vec<BString>>;

pub struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) host: String,
    pub(crate) version: (u32, u32),
    pub(crate) headers: HeaderMap,
    pub(crate) body: *mut Reader,
}

impl Request {
    /// SAFETY:
    /// body must have been created with a new reader function
    pub(crate) unsafe fn new(
        method: String,
        path: String,
        host: String,
        version: (u32, u32),
        headers: HeaderMap,
        body: *mut Reader,
    ) -> *mut Request {
        let req = Request {
            method,
            path,
            host,
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
\tpath: {:?}
\thost: {:?}
\tversion: {:?}
\theaders: {:?}
        }}",
            self.method, self.path, self.host, self.version, self.headers
        )
    }
}

/// Caller must ensure the following
/// 1) Request must be convertible to a reference
/// 2) data must be convertible to a slice of len bytes
/// 3) res must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_read_body(
    request: *mut Request,
    data: *mut u8,
    len: usize,
    res: *mut HttpResult,
) {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );

    // SAFETY: Request is convertible to a reference
    let req = unsafe { request.as_mut_unchecked() };

    // SAFETY:
    // data is convertible to a slice of len bytes
    // read is convertible to a reference
    unsafe {
        http_bio_read(req.body, data, len, res);
    }
}

/// Caller must ensure the following
/// 1) Request must be convertible to a reference
/// 2) string must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_method(request: *const Request, string: *mut Buffer) {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );
    assert!(
        string.is_sane(),
        "{}: Method buffer is not convertible to a reference",
        function!()
    );

    // SAFETY: Request is convertible to a reference
    let req = unsafe { request.as_ref_unchecked() };

    let str = unsafe { string.as_mut_unchecked() };
    assert!(
        str.is_sane(),
        "{}: Method buffer is not convertible to a slice",
        function!()
    );

    str.copy_slice(req.method.as_bytes());
}

/// Caller must ensure the following
/// 1) Request must be convertible to a reference
/// 2) string must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_path(request: *const Request, string: *mut Buffer) {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );

    assert!(
        string.is_sane(),
        "{}: Path buffer is not convertible to a reference",
        function!()
    );

    // SAFETY: Request is convertible to a reference
    let req = unsafe { request.as_ref_unchecked() };

    let str = unsafe { string.as_mut_unchecked() };
    assert!(
        str.is_sane(),
        "{}: Path buffer is not convertible to a slice",
        function!()
    );

    str.copy_slice(req.path.as_bytes());
}

/// Caller must ensure the following
/// 1) Request must be convertible to a reference
/// 2) string must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_uri(request: *const Request, string: *mut Buffer) {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );

    assert!(
        string.is_sane(),
        "{}: URI buffer is not convertible to a reference",
        function!()
    );

    // SAFETY: Request is convertible to a reference
    let req = unsafe { request.as_ref_unchecked() };

    let string = unsafe { string.as_mut_unchecked() };
    assert!(
        string.is_sane(),
        "{}: URI buffer is not convertible to a slice",
        function!()
    );

    string.clear();
    _ = write!(string, "https://{}{}", req.host, req.path);
}

/// Caller must ensure the following
/// 1) Request must be convertible to a reference
/// 2) Major must be convertible to a reference, or be null
/// 3) Minor must be convertible to a reference, or be null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_version(
    request: *const Request,
    major: *mut u32,
    minor: *mut u32,
) {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );
    assert!(
        major.is_aligned(),
        "{}: Major is not properly aligned",
        function!()
    );
    assert!(
        minor.is_aligned(),
        "{}: Minor is not properly aligned",
        function!()
    );

    // SAFETY: Request is a valid, non-null pointer
    let req = unsafe { request.as_ref_unchecked() };

    // SAFETY: Major is either null, or convertible to a reference
    let major = unsafe { major.as_mut() };

    // SAFETY: Minor is either null, or convertible to a reference
    let minor = unsafe { minor.as_mut() };

    if let Some(m) = major {
        *m = req.version.0;
    }

    if let Some(m) = minor {
        *m = req.version.1;
    }
}

/// Caller must ensure the following:
/// 1) Request must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_header_count(
    request: *const Request,
    name: ConstBuffer,
) -> usize {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );
    assert!(
        name.is_sane(),
        "{}: Header name is not convertible to a slice",
        function!()
    );

    // SAFETY: Request is a valid, non-null pointer
    let req = unsafe { request.as_ref_unchecked() };

    req.headers.get(&*name).map(Vec::len).unwrap_or(0)
}

/// Caller must ensure the following:
/// 1) Request must be convertible to a reference
/// 2) Value must be convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_get_header(
    request: *const Request,
    name: ConstBuffer,
    header_index: usize,
    value: *mut Buffer,
) -> c_int {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );

    assert!(
        value.is_sane(),
        "{}: Header value buffer is not convertible to a reference",
        function!()
    );

    assert!(
        name.is_sane(),
        "{}: Header name buffer is not convertible to a slice",
        function!()
    );

    // SAFETY: Request is convertible to a reference
    let req = unsafe { request.as_ref_unchecked() };

    let value = unsafe { value.as_mut_unchecked() };
    assert!(
        value.is_sane(),
        "{}: Header value buffer is not convertible to a slice",
        function!()
    );

    let Some(headers) = req.headers.get(&*name) else {
        return 1;
    };

    let Some(header) = headers.get(header_index) else {
        return 2;
    };

    value.copy_slice(header);

    0
}

/// SAFETY:
/// Request must have been made with new
/// Request must not be used after the call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_request(request: *mut Request) {
    assert!(
        request.is_sane(),
        "{}: Request is not convertible to a reference",
        function!()
    );

    unsafe {
        Request::delete(request);
    }
}
