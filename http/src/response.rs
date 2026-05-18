use bstr::BString;

pub mod status_codes {
    use std::ffi::c_char;

    pub const CONTINUE: u16 = 100;
    pub const SWITCHING_PROTOCOLS: u16 = 101;
    pub const PROCESSING: u16 = 102;
    pub const EARLY_HINTS: u16 = 103;

    pub const OK: u16 = 200;
    pub const CREATED: u16 = 201;
    pub const ACCEPTED: u16 = 202;
    pub const NON_AUTHORITATIVE_INFORMATION: u16 = 203;
    pub const NO_CONTENT: u16 = 204;
    pub const RESET_CONTENT: u16 = 205;
    pub const PARTIAL_CONTENT: u16 = 206;

    pub const MULTIPLE_CHOICES: u16 = 300;
    pub const MOVED_PERMANENTLY: u16 = 301;
    pub const FOUND: u16 = 302;
    pub const SEE_OTHER: u16 = 303;
    pub const NOT_MODIFIED: u16 = 304;
    pub const TEMPORARY_REDIRECT: u16 = 307;
    pub const PERMANENT_REDIRECT: u16 = 308;

    pub const BAD_REQUEST: u16 = 400;
    pub const UNAUTHORIZED: u16 = 401;
    pub const PAYMENT_REQUIRED: u16 = 402;
    pub const FORBIDDEN: u16 = 403;
    pub const NOT_FOUND: u16 = 404;
    pub const METHOD_NOT_ALLOWED: u16 = 405;
    pub const NOT_ACCEPTABLE: u16 = 406;
    pub const PROXY_AUTHENTICATION_REQUIRED: u16 = 407;
    pub const REQUEST_TIMEOUT: u16 = 408;
    pub const CONFLICT: u16 = 409;
    pub const GONE: u16 = 410;
    pub const LENGTH_REQUIRED: u16 = 411;
    pub const PRECONDITION_FAILED: u16 = 412;
    pub const CONTENT_TOO_LARGE: u16 = 413;
    pub const URITOO_LONG: u16 = 414;
    pub const UNSUPPORTED_MEDIA_TYPE: u16 = 415;
    pub const RANGE_NOT_SATISFIABLE: u16 = 416;
    pub const EXPECTATION_FAILED: u16 = 417;
    pub const IM_ATEAPOT: u16 = 418;
    pub const MISDIRECTED_REQUEST: u16 = 421;
    pub const UPGRADE_REQUIRED: u16 = 426;
    pub const PRECONDITION_REQUIRED: u16 = 428;
    pub const TOO_MANY_REQUESTS: u16 = 429;
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: u16 = 431;
    pub const UNAVAILABLE_FOR_LEGAL_REASONS: u16 = 451;

    pub const INTERNAL_SERVER_ERROR: u16 = 500;
    pub const NOT_IMPLEMENTED: u16 = 501;
    pub const BAD_GATEWAY: u16 = 502;
    pub const SERVICE_UNAVAILABLE: u16 = 503;
    pub const GATEWAY_TIMEOUT: u16 = 504;
    pub const HTTPVERSION_NOT_SUPPORTED: u16 = 505;
    pub const VARIANT_ALSO_NEGOTIATES: u16 = 506;
    pub const NOT_EXTENDED: u16 = 510;
    pub const NETWORK_AUTHENTICATION_REQUIRED: u16 = 511;

    #[unsafe(no_mangle)]
    pub extern "C" fn get_reason_phrase(code: u16) -> *const c_char {
        match code {
            CONTINUE => c"Continue".as_ptr(),
            SWITCHING_PROTOCOLS => c"Switching Protocols".as_ptr(),
            PROCESSING => c"Processing".as_ptr(),
            EARLY_HINTS => c"Early Hints".as_ptr(),

            OK => c"OK".as_ptr(),
            CREATED => c"Created".as_ptr(),
            ACCEPTED => c"Accepted".as_ptr(),
            NON_AUTHORITATIVE_INFORMATION => c"Non-Authoritative Information".as_ptr(),
            NO_CONTENT => c"No Content".as_ptr(),
            RESET_CONTENT => c"Reset Content".as_ptr(),
            PARTIAL_CONTENT => c"Partial Content".as_ptr(),

            MULTIPLE_CHOICES => c"Multiple Choices".as_ptr(),
            MOVED_PERMANENTLY => c"Moved Permanently".as_ptr(),
            FOUND => c"Found".as_ptr(),
            SEE_OTHER => c"See Other".as_ptr(),
            NOT_MODIFIED => c"Not Modified".as_ptr(),
            TEMPORARY_REDIRECT => c"Temporary Redirect".as_ptr(),
            PERMANENT_REDIRECT => c"Permanent Redirect".as_ptr(),

            BAD_REQUEST => c"Bad Request".as_ptr(),
            UNAUTHORIZED => c"Unauthorized".as_ptr(),
            PAYMENT_REQUIRED => c"Payment Required".as_ptr(),
            FORBIDDEN => c"Forbidden".as_ptr(),
            NOT_FOUND => c"Not Found".as_ptr(),
            METHOD_NOT_ALLOWED => c"Method Not Allowed".as_ptr(),
            NOT_ACCEPTABLE => c"Not Acceptable".as_ptr(),
            PROXY_AUTHENTICATION_REQUIRED => c"Proxy Authentication Required".as_ptr(),
            REQUEST_TIMEOUT => c"Request Timeout".as_ptr(),
            CONFLICT => c"Conflict".as_ptr(),
            GONE => c"Gone".as_ptr(),
            LENGTH_REQUIRED => c"Length Required".as_ptr(),
            PRECONDITION_FAILED => c"Precondition Failed".as_ptr(),
            CONTENT_TOO_LARGE => c"Content Too Large".as_ptr(),
            URITOO_LONG => c"URI Too Long".as_ptr(),
            UNSUPPORTED_MEDIA_TYPE => c"Unsupported Media Type".as_ptr(),
            RANGE_NOT_SATISFIABLE => c"Range Not Satisfiable".as_ptr(),
            EXPECTATION_FAILED => c"Expectation Failed".as_ptr(),
            IM_ATEAPOT => c"I'm A Teapot".as_ptr(),
            MISDIRECTED_REQUEST => c"Misdirected Request".as_ptr(),
            UPGRADE_REQUIRED => c"Upgrade Required".as_ptr(),
            PRECONDITION_REQUIRED => c"Precondition Required".as_ptr(),
            TOO_MANY_REQUESTS => c"Too Many Requests".as_ptr(),
            REQUEST_HEADER_FIELDS_TOO_LARGE => c"Request Header Fields Too Large".as_ptr(),
            UNAVAILABLE_FOR_LEGAL_REASONS => c"Unavailable For Legal Reasons".as_ptr(),

            INTERNAL_SERVER_ERROR => c"Internal Server Error".as_ptr(),
            NOT_IMPLEMENTED => c"Not Implemented".as_ptr(),
            BAD_GATEWAY => c"Bad Gateway".as_ptr(),
            SERVICE_UNAVAILABLE => c"Service Unavailable".as_ptr(),
            GATEWAY_TIMEOUT => c"Gateway Timeout".as_ptr(),
            HTTPVERSION_NOT_SUPPORTED => c"HTTP Version Not Supported".as_ptr(),
            VARIANT_ALSO_NEGOTIATES => c"Variant Also Negotiates".as_ptr(),
            NOT_EXTENDED => c"Not Extended".as_ptr(),
            NETWORK_AUTHENTICATION_REQUIRED => c"Network Authentication Required".as_ptr(),
            _ => c"".as_ptr(),
        }
    }
}

pub use status_codes::*;

use crate::{
    HeaderMap, IsSane,
    bio::{Writer, http_bio_write, http_destroy_writer},
    buffer::ConstBuffer,
    function,
    result::{HttpResult, set_ok},
};

/// SAFETY:
/// 1) writer must be convertible to a reference
/// 2) res must be convertible to a reference
/// 3) res must be a result to a usize
pub type StatusFn =
    unsafe extern "C" fn(writer: *mut ResponseWriter, code: u16, res: *mut HttpResult);

/// Caller must ensure the following:
/// 1) writer is convertible to a reference
/// 2) data is convertible to a slice of len bytes
/// 3) res is convertible to a reference
/// 4) res must be a result to a usize
pub type WriteFn =
    unsafe extern "C" fn(writer: *mut Writer, data: *const u8, len: usize, res: *mut HttpResult);

pub struct ResponseWriter {
    pub(crate) writer: *mut Writer,
    pub(crate) headers: HeaderMap,
    write_fn: WriteFn,
    status_fn: StatusFn,
    written: bool,
}

/// SAFETY:
/// 1) writer must have been created from http_new_writer function
/// 2) WriteFn's safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_new_response_writer(
    writer: *mut Writer,
    status_fn: StatusFn,
) -> *mut ResponseWriter {
    unsafe { http_custom_response_writer(writer, status_fn, http_bio_write) }
}

/// SAFETY:
/// 1) writer must have been created from http_new_writer function
/// 2) WriteFn's safety requirements
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_custom_response_writer(
    writer: *mut Writer,
    status_fn: StatusFn,
    write_fn: WriteFn,
) -> *mut ResponseWriter {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );

    let w = Box::new(ResponseWriter {
        writer,
        headers: HeaderMap::new(),
        status_fn,
        write_fn,
        written: false,
    });

    Box::into_raw(w)
}

/// Caller must ensure the following:
/// 1) writer is convertible to a reference
/// 2) data is convertible to a slice of len bytes
/// 3) res is convertible to a reference
/// 4) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write(
    writer: *mut ResponseWriter,
    data: *const u8,
    len: usize,
    res: *mut HttpResult,
) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        data.is_sane(),
        "{}: Data is not convertible to a slice",
        function!()
    );
    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    // SAFETY: Writer is convertible to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY: written is convertible to a reference
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    if !writer.written {
        // SAFETY:
        // Writer is convertible to a reference, and status code is obviously a status code
        // Written is convertible to a reference
        unsafe { http_write_status(writer, OK, res) };

        if !res.is_ok {
            return;
        }
    }

    unsafe { (writer.write_fn)(writer.writer, data.cast(), len, res) }
}

/// Caller must ensure the following:
/// 1) Writer is convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_add_header(
    writer: *mut ResponseWriter,
    name: ConstBuffer,
    value: ConstBuffer,
) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        name.is_sane(),
        "{}: Name is not convertible to a slice",
        function!()
    );
    assert!(
        value.is_sane(),
        "{}: Value is not convertible to a slice",
        function!()
    );

    // SAFETY: Writer is convertible to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    if name.is_empty() {
        return;
    }

    let mut name = BString::from(&*name);
    name.make_ascii_lowercase();

    let headers = &mut writer.headers;

    if !headers.contains_key(&name) || !value.is_empty() {
        headers
            .entry(name)
            .or_insert_with(|| Vec::with_capacity(4))
            .push(BString::from(&*value));
    }
}

/// Caller must ensure the following:
/// 1) writer is convertible to a reference
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_remove_header(writer: *mut ResponseWriter, header: ConstBuffer) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        header.is_sane(),
        "{}: Header is not convertible to a slice",
        function!()
    );

    // SAFETY: Writer is convertible to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    let headers = &mut writer.headers;

    headers.remove(&*header);
}

/// Caller must ensure the following:
/// 1) writer is convertible to a reference
/// 2) status must be a valid status code (technically, you can put any u16 in here)
/// 3) res must be convertible to a reference
/// 4) res must be a result to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write_status(
    writer: *mut ResponseWriter,
    status: u16,
    res: *mut HttpResult,
) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );
    assert!(
        res.is_sane(),
        "{}: Result is not convertible to a reference",
        function!()
    );

    // SAFETY: Writer is convertible to a reference
    let writer = unsafe { writer.as_mut_unchecked() };

    // SAFETY: res is convertible to a reference
    let res = unsafe { res.as_mut_unchecked() };
    assert!(
        res.err.is_sane(),
        "{}: Result error is not convertible to a slice",
        function!()
    );

    if writer.written {
        unsafe {
            set_ok(res, 0usize, function!());
        };
        return;
    }

    // SAFETY:
    // writer is a reference
    // res can be converted to a reference
    unsafe { (writer.status_fn)(writer, status, res) };

    if !res.is_ok {
        return;
    }

    writer.written = true;
}

/// SAFETY:
/// Writer must be a writer created from http_new_response_writer
/// Writer must not be used after call to this function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_response_writer(writer: *mut ResponseWriter) {
    assert!(
        writer.is_sane(),
        "{}: Writer is not convertible to a reference",
        function!()
    );

    // SAFETY: Writer was created from http_new_response_writer, so the pointer
    // is from Box::into_raw
    let w = unsafe { Box::from_raw(writer) };

    // SAFETY: Writer is a reference, and writer was created from the http_new_writer method
    unsafe {
        http_destroy_writer(w.writer);
    }

    drop(w);
}
